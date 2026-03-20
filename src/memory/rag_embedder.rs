use anyhow::anyhow;
use fastembed::{
    EmbeddingModel, InitOptions, InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles,
    UserDefinedEmbeddingModel,
};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

const RAG_MODEL_ENV_VAR: &str = "ARKNIGHTS_RAG_MODEL";
const PROJECT_FASTEMBED_CACHE_DIR: &str = "models/fastembed";

static EMBEDDER: LazyLock<Mutex<Option<EmbedderState>>> = LazyLock::new(|| Mutex::new(None));

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalModelBundlePaths {
    onnx_file: PathBuf,
    tokenizer_file: PathBuf,
    config_file: PathBuf,
    special_tokens_map_file: PathBuf,
    tokenizer_config_file: PathBuf,
}

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

pub async fn embed_chat_history(
    config: RagRuntimeConfig,
    user_content: &str,
    assistant_content: &str,
) -> anyhow::Result<Vec<f32>> {
    let text = build_chat_history_embedding_input(user_content, assistant_content);
    embed_text(config, text).await
}

pub async fn embed_text(config: RagRuntimeConfig, text: String) -> anyhow::Result<Vec<f32>> {
    #[cfg(test)]
    if let Some(embedding) = test_embedding_result()? {
        return embedding;
    }

    tokio::task::spawn_blocking(move || {
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
    })
    .await?
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
#[derive(Clone, Debug)]
enum TestEmbeddingMode {
    Success(Vec<f32>),
    Fail(String),
}

#[cfg(test)]
static TEST_EMBEDDING_MODE: LazyLock<Mutex<Option<TestEmbeddingMode>>> =
    LazyLock::new(|| Mutex::new(None));
#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[cfg(test)]
fn test_embedding_result() -> anyhow::Result<Option<anyhow::Result<Vec<f32>>>> {
    let mode = TEST_EMBEDDING_MODE
        .lock()
        .map_err(|_| anyhow!("test rag embedder mutex poisoned"))?
        .clone();

    Ok(mode.map(|mode| match mode {
        TestEmbeddingMode::Success(embedding) => Ok(embedding),
        TestEmbeddingMode::Fail(message) => Err(anyhow!(message)),
    }))
}

#[cfg(test)]
pub fn set_test_embedding_success(embedding: Vec<f32>) {
    *TEST_EMBEDDING_MODE.lock().unwrap() = Some(TestEmbeddingMode::Success(embedding));
}

#[cfg(test)]
pub fn set_test_embedding_failure(message: &str) {
    *TEST_EMBEDDING_MODE.lock().unwrap() = Some(TestEmbeddingMode::Fail(message.to_string()));
}

#[cfg(test)]
pub fn clear_test_embedding_mode() {
    *TEST_EMBEDDING_MODE.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rag_config_defaults_to_disabled_when_model_is_unset() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::remove_var(RAG_MODEL_ENV_VAR);
        }

        assert_eq!(RagConfig::from_env().unwrap(), RagConfig::Disabled);
    }

    #[test]
    fn rag_config_defaults_to_disabled_when_model_is_empty() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var(RAG_MODEL_ENV_VAR, "   ");
        }

        assert_eq!(RagConfig::from_env().unwrap(), RagConfig::Disabled);
    }

    #[test]
    fn rag_config_reads_supported_model_and_project_cache_dir() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var(RAG_MODEL_ENV_VAR, "BAAI/bge-small-en-v1.5");
        }

        let config = RagConfig::from_env().unwrap();
        let enabled = match config {
            RagConfig::Enabled(config) => config,
            RagConfig::Disabled => panic!("expected enabled config"),
        };

        assert_eq!(enabled.model, RagModel::BgeSmallEnV15);
        assert_eq!(enabled.cache_dir, project_fastembed_cache_dir());
    }

    #[test]
    fn rag_config_rejects_invalid_model() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var(RAG_MODEL_ENV_VAR, "unsupported-model");
        }

        let err = RagConfig::from_env().unwrap_err();
        assert!(err.to_string().contains("unsupported ARKNIGHTS_RAG_MODEL"));
    }

    #[test]
    fn build_chat_history_embedding_input_formats_pair() {
        let text = build_chat_history_embedding_input("你好", "世界");
        assert_eq!(text, "User:\n你好\n\nAssistant:\n世界");
    }

    #[test]
    fn project_fastembed_cache_dir_is_under_repo_root() {
        let cache_dir = project_fastembed_cache_dir();
        assert!(cache_dir.ends_with("models/fastembed"));
        assert!(cache_dir.starts_with(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn detect_local_model_bundle_accepts_root_model_layout() {
        let dir = unique_temp_dir("local-bundle-ok");
        write_bundle_files(&dir, true);

        let bundle = detect_local_model_bundle(&dir).unwrap().unwrap();
        assert_eq!(bundle.onnx_file, dir.join("model.onnx"));

        cleanup_dir(&dir);
    }

    #[test]
    fn detect_local_model_bundle_accepts_nested_onnx_layout() {
        let dir = unique_temp_dir("local-bundle-nested");
        write_bundle_files(&dir, false);

        let bundle = detect_local_model_bundle(&dir).unwrap().unwrap();
        assert_eq!(bundle.onnx_file, dir.join("onnx").join("model.onnx"));

        cleanup_dir(&dir);
    }

    #[test]
    fn detect_local_model_bundle_reports_missing_tokenizer_files() {
        let dir = unique_temp_dir("local-bundle-missing");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("model.onnx"), b"dummy").unwrap();

        let err = detect_local_model_bundle(&dir).unwrap_err();
        assert!(
            err.to_string()
                .contains("local rag model bundle incomplete")
        );
        assert!(err.to_string().contains("tokenizer.json"));
        assert!(err.to_string().contains("config.json"));

        cleanup_dir(&dir);
    }

    #[tokio::test]
    async fn embed_text_uses_test_override_without_loading_model() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        clear_test_embedding_mode();
        set_test_embedding_success(vec![0.1; 384]);

        let embedding = embed_text(
            RagRuntimeConfig {
                model: RagModel::BgeSmallEnV15,
                cache_dir: project_fastembed_cache_dir(),
            },
            "hello".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(embedding.len(), 384);

        clear_test_embedding_mode();
    }

    #[tokio::test]
    async fn embed_text_returns_test_failure_override() {
        let _guard = TEST_ENV_LOCK.lock().unwrap();
        clear_test_embedding_mode();
        set_test_embedding_failure("forced failure");

        let err = embed_text(
            RagRuntimeConfig {
                model: RagModel::BgeSmallEnV15,
                cache_dir: project_fastembed_cache_dir(),
            },
            "hello".to_string(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("forced failure"));

        clear_test_embedding_mode();
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("arknights_rag_embedder_{prefix}_{nanos}"))
    }

    fn write_bundle_files(dir: &Path, root_model: bool) {
        fs::create_dir_all(dir).unwrap();
        let onnx_path = if root_model {
            dir.join("model.onnx")
        } else {
            let onnx_dir = dir.join("onnx");
            fs::create_dir_all(&onnx_dir).unwrap();
            onnx_dir.join("model.onnx")
        };

        fs::write(onnx_path, b"dummy").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.join("config.json"), br#"{"pad_token_id":0}"#).unwrap();
        fs::write(dir.join("special_tokens_map.json"), b"{}").unwrap();
        fs::write(
            dir.join("tokenizer_config.json"),
            br#"{"model_max_length":512,"pad_token":"[PAD]"}"#,
        )
        .unwrap();
    }

    fn cleanup_dir(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }
}
