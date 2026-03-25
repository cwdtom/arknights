use super::*;
use crate::test_support;
use std::fs;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn rag_embedder_source_keeps_only_external_test_module_gate() {
    let source = include_str!("rag_embedder.rs");
    assert_eq!(source.matches("#[cfg(test)]").count(), 1);
}

#[test]
fn rag_config_defaults_to_disabled_when_model_is_unset() {
    let _guard = test_support::lock_test_env();
    unsafe {
        std::env::remove_var(RAG_MODEL_ENV_VAR);
    }

    assert_eq!(RagConfig::from_env().unwrap(), RagConfig::Disabled);
}

#[test]
fn rag_config_defaults_to_disabled_when_model_is_empty() {
    let _guard = test_support::lock_test_env();
    unsafe {
        std::env::set_var(RAG_MODEL_ENV_VAR, "   ");
    }

    assert_eq!(RagConfig::from_env().unwrap(), RagConfig::Disabled);
}

#[test]
fn rag_config_reads_supported_model_and_project_cache_dir() {
    let _guard = test_support::lock_test_env();
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
    let _guard = test_support::lock_test_env();
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
async fn embed_text_uses_backend_override_without_loading_model() {
    let _guard = test_support::lock_test_env();
    let backend = FakeRagEmbedder::success(vec![0.1; 384]);
    let embedding = embed_text_with_backend(
        RagRuntimeConfig {
            model: RagModel::BgeSmallEnV15,
            cache_dir: project_fastembed_cache_dir(),
        },
        "hello".to_string(),
        &backend,
    )
    .await
    .unwrap();
    assert_eq!(embedding, vec![0.1; 384]);
}

#[tokio::test]
async fn embed_text_returns_backend_failure() {
    let _guard = test_support::lock_test_env();
    let backend = FakeRagEmbedder::failure("forced failure");
    let err = embed_text_with_backend(
        RagRuntimeConfig {
            model: RagModel::BgeSmallEnV15,
            cache_dir: project_fastembed_cache_dir(),
        },
        "hello".to_string(),
        &backend,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("forced failure"));
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

struct FakeRagEmbedder {
    result: Mutex<Option<anyhow::Result<Vec<f32>>>>,
}

impl FakeRagEmbedder {
    fn success(embedding: Vec<f32>) -> Self {
        Self {
            result: Mutex::new(Some(Ok(embedding))),
        }
    }

    fn failure(message: &str) -> Self {
        Self {
            result: Mutex::new(Some(Err(anyhow::anyhow!(message.to_string())))),
        }
    }
}

#[async_trait::async_trait]
impl RagEmbeddingBackend for FakeRagEmbedder {
    async fn embed_text(
        &self,
        _config: RagRuntimeConfig,
        _text: String,
    ) -> anyhow::Result<Vec<f32>> {
        self.result
            .lock()
            .unwrap()
            .take()
            .expect("fake rag embedder should be called once")
    }
}
