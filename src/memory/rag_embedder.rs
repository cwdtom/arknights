use anyhow::anyhow;
use fastembed::{
    EmbeddingModel, InitOptions, InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles,
    UserDefinedEmbeddingModel,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

const RAG_MODEL_ENV_VAR: &str = "ARKNIGHTS_RAG_MODEL";
const PROJECT_FASTEMBED_CACHE_DIR: &str = "models/fastembed";

static EMBEDDER: LazyLock<Mutex<Option<EmbedderState>>> = LazyLock::new(|| Mutex::new(None));
static DEFAULT_EMBEDDER: LazyLock<SharedRagEmbeddingBackend> =
    LazyLock::new(|| Arc::new(FastembedBackend));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RagConfig {
    Disabled,
    Enabled(RagRuntimeConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RagRuntimeConfig {
    pub model: RagModel,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RagModel {
    BgeSmallEnV15,
    BgeSmallZhV15,
}

struct EmbedderState {
    config: RagRuntimeConfig,
    embedder: TextEmbedding,
}

pub(crate) type SharedRagEmbeddingBackend = Arc<dyn RagEmbeddingBackend>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalModelBundlePaths {
    onnx_file: PathBuf,
    tokenizer_file: PathBuf,
    config_file: PathBuf,
    special_tokens_map_file: PathBuf,
    tokenizer_config_file: PathBuf,
}

#[async_trait::async_trait]
pub(crate) trait RagEmbeddingBackend: Send + Sync {
    async fn embed_text(&self, config: RagRuntimeConfig, text: String) -> anyhow::Result<Vec<f32>>;
}

struct FastembedBackend;

impl RagConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let value = match std::env::var(RAG_MODEL_ENV_VAR) {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => return Ok(Self::Disabled),
            Err(err) => return Err(anyhow!("read {RAG_MODEL_ENV_VAR} failed: {err}")),
        };

        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(Self::Disabled);
        }

        Ok(Self::Enabled(RagRuntimeConfig {
            model: RagModel::try_from(trimmed)?,
            cache_dir: project_fastembed_cache_dir(),
        }))
    }
}

impl RagRuntimeConfig {
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

impl RagModel {
    pub fn model_name(self) -> &'static str {
        match self {
            Self::BgeSmallEnV15 => "BAAI/bge-small-en-v1.5",
            Self::BgeSmallZhV15 => "BAAI/bge-small-zh-v1.5",
        }
    }

    pub fn dimension(self) -> usize {
        match self {
            Self::BgeSmallEnV15 => 384,
            Self::BgeSmallZhV15 => 512,
        }
    }

    fn as_fastembed_model(self) -> EmbeddingModel {
        match self {
            Self::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
            Self::BgeSmallZhV15 => EmbeddingModel::BGESmallZHV15,
        }
    }

    fn pooling(self) -> Pooling {
        match self {
            Self::BgeSmallEnV15 | Self::BgeSmallZhV15 => Pooling::Cls,
        }
    }
}

pub fn build_chat_history_embedding_input(user_content: &str, assistant_content: &str) -> String {
    format!("User:\n{user_content}\n\nAssistant:\n{assistant_content}")
}

pub fn project_fastembed_cache_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROJECT_FASTEMBED_CACHE_DIR)
}

pub(crate) fn default_backend() -> SharedRagEmbeddingBackend {
    Arc::clone(&DEFAULT_EMBEDDER)
}

pub(crate) async fn embed_chat_history_with_backend(
    config: RagRuntimeConfig,
    user_content: &str,
    assistant_content: &str,
    backend: &dyn RagEmbeddingBackend,
) -> anyhow::Result<Vec<f32>> {
    let text = build_chat_history_embedding_input(user_content, assistant_content);
    embed_text_with_backend(config, text, backend).await
}

pub(crate) async fn embed_text_with_backend(
    config: RagRuntimeConfig,
    text: String,
    backend: &dyn RagEmbeddingBackend,
) -> anyhow::Result<Vec<f32>> {
    backend.embed_text(config, text).await
}

#[async_trait::async_trait]
impl RagEmbeddingBackend for FastembedBackend {
    async fn embed_text(&self, config: RagRuntimeConfig, text: String) -> anyhow::Result<Vec<f32>> {
        tokio::task::spawn_blocking(move || embed_text_blocking(config, text)).await?
    }
}

fn embed_text_blocking(config: RagRuntimeConfig, text: String) -> anyhow::Result<Vec<f32>> {
    std::fs::create_dir_all(config.cache_dir())
        .map_err(|err| anyhow!("create rag cache dir failed: {err}"))?;

    let mut embedder_state = EMBEDDER
        .lock()
        .map_err(|_| anyhow!("rag embedder mutex poisoned"))?;

    let needs_init = embedder_state
        .as_ref()
        .map(|state| state.config != config)
        .unwrap_or(true);

    if needs_init {
        let embedder = build_embedder(&config)?;
        *embedder_state = Some(EmbedderState {
            config: config.clone(),
            embedder,
        });
    }

    let embeddings = embedder_state
        .as_mut()
        .ok_or_else(|| anyhow!("rag embedder not initialized"))?
        .embedder
        .embed(vec![text], None)?;

    let embedding = embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("fastembed returned no embedding"))?;

    if embedding.len() != config.model.dimension() {
        return Err(anyhow!(
            "rag embedding dimension mismatch: expected {}, got {}",
            config.model.dimension(),
            embedding.len()
        ));
    }

    Ok(embedding)
}

fn build_embedder(config: &RagRuntimeConfig) -> anyhow::Result<TextEmbedding> {
    if let Some(bundle) = detect_local_model_bundle(config.cache_dir())? {
        return build_local_embedder(config, &bundle);
    }

    TextEmbedding::try_new(
        InitOptions::new(config.model.as_fastembed_model())
            .with_cache_dir(config.cache_dir.clone()),
    )
}

fn build_local_embedder(
    config: &RagRuntimeConfig,
    bundle: &LocalModelBundlePaths,
) -> anyhow::Result<TextEmbedding> {
    let model = UserDefinedEmbeddingModel::new(
        std::fs::read(&bundle.onnx_file)
            .map_err(|err| anyhow!("read local onnx file failed: {err}"))?,
        TokenizerFiles {
            tokenizer_file: std::fs::read(&bundle.tokenizer_file)
                .map_err(|err| anyhow!("read local tokenizer.json failed: {err}"))?,
            config_file: std::fs::read(&bundle.config_file)
                .map_err(|err| anyhow!("read local config.json failed: {err}"))?,
            special_tokens_map_file: std::fs::read(&bundle.special_tokens_map_file)
                .map_err(|err| anyhow!("read local special_tokens_map.json failed: {err}"))?,
            tokenizer_config_file: std::fs::read(&bundle.tokenizer_config_file)
                .map_err(|err| anyhow!("read local tokenizer_config.json failed: {err}"))?,
        },
    )
    .with_pooling(config.model.pooling());

    TextEmbedding::try_new_from_user_defined(
        model,
        InitOptionsUserDefined::from(InitOptions::new(config.model.as_fastembed_model())),
    )
}

fn detect_local_model_bundle(cache_dir: &Path) -> anyhow::Result<Option<LocalModelBundlePaths>> {
    let onnx_file = match find_local_onnx_file(cache_dir) {
        Some(path) => path,
        None => return Ok(None),
    };

    let bundle = LocalModelBundlePaths {
        onnx_file,
        tokenizer_file: cache_dir.join("tokenizer.json"),
        config_file: cache_dir.join("config.json"),
        special_tokens_map_file: cache_dir.join("special_tokens_map.json"),
        tokenizer_config_file: cache_dir.join("tokenizer_config.json"),
    };

    let missing = missing_bundle_files(&bundle);
    if !missing.is_empty() {
        return Err(anyhow!(
            "local rag model bundle incomplete under {}: missing {}",
            cache_dir.display(),
            missing.join(", ")
        ));
    }

    Ok(Some(bundle))
}

fn find_local_onnx_file(cache_dir: &Path) -> Option<PathBuf> {
    let nested = cache_dir.join("onnx").join("model.onnx");
    if nested.is_file() {
        return Some(nested);
    }

    let root = cache_dir.join("model.onnx");
    if root.is_file() {
        return Some(root);
    }

    None
}

fn missing_bundle_files(bundle: &LocalModelBundlePaths) -> Vec<&'static str> {
    let mut missing = Vec::new();

    if !bundle.tokenizer_file.is_file() {
        missing.push("tokenizer.json");
    }
    if !bundle.config_file.is_file() {
        missing.push("config.json");
    }
    if !bundle.special_tokens_map_file.is_file() {
        missing.push("special_tokens_map.json");
    }
    if !bundle.tokenizer_config_file.is_file() {
        missing.push("tokenizer_config.json");
    }

    missing
}

impl TryFrom<&str> for RagModel {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim() {
            "BAAI/bge-small-en-v1.5" | "bge-small-en-v1.5" => Ok(Self::BgeSmallEnV15),
            "BAAI/bge-small-zh-v1.5" | "bge-small-zh-v1.5" => Ok(Self::BgeSmallZhV15),
            _ => Err(anyhow!("unsupported {RAG_MODEL_ENV_VAR}: {value}")),
        }
    }
}

#[cfg(test)]
#[path = "rag_embedder_tests.rs"]
mod tests;
