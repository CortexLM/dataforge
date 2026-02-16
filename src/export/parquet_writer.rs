//! Parquet writer for SWE-bench compatible dataset format.
//!
//! Produces Parquet files matching the princeton-nlp/SWE-bench schema plus
//! extra fields from swe-forge (difficulty, quality_score, etc.).

use std::path::Path;
use std::sync::Arc;

use arrow::array::{ArrayRef, Float64Builder, StringArray, StringBuilder, UInt8Builder};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::swe::SweTask;

/// Schema matching SWE-bench format + swe-forge extensions.
pub fn swe_bench_schema() -> Schema {
    Schema::new(vec![
        Field::new("instance_id", DataType::Utf8, false),
        Field::new("repo", DataType::Utf8, false),
        Field::new("base_commit", DataType::Utf8, false),
        Field::new("patch", DataType::Utf8, false),
        Field::new("test_patch", DataType::Utf8, true),
        Field::new("problem_statement", DataType::Utf8, false),
        Field::new("hints_text", DataType::Utf8, true),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("version", DataType::Utf8, true),
        Field::new("FAIL_TO_PASS", DataType::Utf8, false),
        Field::new("PASS_TO_PASS", DataType::Utf8, false),
        Field::new("environment_setup_commit", DataType::Utf8, true),
        // swe-forge extensions
        Field::new("language", DataType::Utf8, false),
        Field::new("difficulty", DataType::Utf8, false),
        Field::new("difficulty_score", DataType::UInt8, false),
        Field::new("quality_score", DataType::Float64, true),
    ])
}

/// Convert a batch of SweTask into an Arrow RecordBatch.
pub fn tasks_to_record_batch(tasks: &[SweTask]) -> anyhow::Result<RecordBatch> {
    let schema = Arc::new(swe_bench_schema());

    let mut instance_id = StringBuilder::new();
    let mut repo = StringBuilder::new();
    let mut base_commit = StringBuilder::new();
    let mut patch = StringBuilder::new();
    let mut test_patch = StringBuilder::new();
    let mut problem_statement = StringBuilder::new();
    let mut hints_text = StringBuilder::new();
    let mut created_at = StringBuilder::new();
    let mut version = StringBuilder::new();
    let mut fail_to_pass = StringBuilder::new();
    let mut pass_to_pass = StringBuilder::new();
    let mut env_setup_commit = StringBuilder::new();
    let mut language = StringBuilder::new();
    let mut difficulty = StringBuilder::new();
    let mut difficulty_score = UInt8Builder::new();
    let mut quality_score = Float64Builder::new();

    for task in tasks {
        instance_id.append_value(&task.id);
        repo.append_value(&task.repo);
        base_commit.append_value(&task.base_commit);
        patch.append_value(&task.patch);
        if task.test_patch.is_empty() {
            test_patch.append_null();
        } else {
            test_patch.append_value(&task.test_patch);
        }
        problem_statement.append_value(&task.prompt);

        let hints = task.original_pr_body.clone();
        if hints.is_empty() {
            hints_text.append_null();
        } else {
            hints_text.append_value(&hints);
        }

        created_at.append_value(task.created_at.to_rfc3339());

        let ver = task.meta.get("version").cloned().unwrap_or_default();
        if ver.is_empty() {
            version.append_null();
        } else {
            version.append_value(&ver);
        }

        let f2p = serde_json::to_string(&task.fail_to_pass).unwrap_or_else(|_| "[]".to_string());
        fail_to_pass.append_value(&f2p);
        let p2p = serde_json::to_string(&task.pass_to_pass).unwrap_or_else(|_| "[]".to_string());
        pass_to_pass.append_value(&p2p);

        let env_commit = task
            .meta
            .get("environment_setup_commit")
            .cloned()
            .unwrap_or_default();
        if env_commit.is_empty() {
            env_setup_commit.append_null();
        } else {
            env_setup_commit.append_value(&env_commit);
        }

        language.append_value(&task.language);

        let diff_label =
            task.meta
                .get("difficulty")
                .cloned()
                .unwrap_or_else(|| match task.difficulty_score {
                    0..=1 => "easy".to_string(),
                    2 => "medium".to_string(),
                    _ => "hard".to_string(),
                });
        difficulty.append_value(&diff_label);
        difficulty_score.append_value(task.difficulty_score);

        match task.quality_score {
            Some(qs) => quality_score.append_value(qs),
            None => quality_score.append_null(),
        }
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(instance_id.finish()),
        Arc::new(repo.finish()),
        Arc::new(base_commit.finish()),
        Arc::new(patch.finish()),
        Arc::new(test_patch.finish()),
        Arc::new(problem_statement.finish()),
        Arc::new(hints_text.finish()),
        Arc::new(created_at.finish()),
        Arc::new(version.finish()),
        Arc::new(fail_to_pass.finish()),
        Arc::new(pass_to_pass.finish()),
        Arc::new(env_setup_commit.finish()),
        Arc::new(language.finish()),
        Arc::new(difficulty.finish()),
        Arc::new(difficulty_score.finish()),
        Arc::new(quality_score.finish()),
    ];

    RecordBatch::try_new(schema, columns)
        .map_err(|e| anyhow::anyhow!("Failed to create RecordBatch: {}", e))
}

/// Write tasks to a Parquet file on disk.
pub fn write_parquet(tasks: &[SweTask], output_path: &Path) -> anyhow::Result<()> {
    if tasks.is_empty() {
        anyhow::bail!("No tasks to write");
    }

    let batch = tasks_to_record_batch(tasks)?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(output_path)?;
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .build();

    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    tracing::info!(
        path = %output_path.display(),
        rows = tasks.len(),
        "Parquet file written"
    );

    Ok(())
}

/// Write tasks to Parquet bytes in memory (for direct HF upload).
pub fn write_parquet_bytes(tasks: &[SweTask]) -> anyhow::Result<Vec<u8>> {
    if tasks.is_empty() {
        anyhow::bail!("No tasks to write");
    }

    let batch = tasks_to_record_batch(tasks)?;

    let mut buf = Vec::new();
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .build();

    let mut writer = ArrowWriter::try_new(&mut buf, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(buf)
}

/// Read tasks from a Parquet file (for loading datasets from HF).
pub fn read_parquet(input_path: &Path) -> anyhow::Result<Vec<SweTask>> {
    use arrow::array::Array;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(input_path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;

    let mut tasks = Vec::new();

    for batch_result in reader {
        let batch = batch_result?;
        let num_rows = batch.num_rows();

        let get_string = |name: &str| -> Vec<Option<String>> {
            batch
                .column_by_name(name)
                .and_then(|col| col.as_any().downcast_ref::<StringArray>())
                .map(|arr| {
                    (0..num_rows)
                        .map(|i| {
                            if arr.is_null(i) {
                                None
                            } else {
                                Some(arr.value(i).to_string())
                            }
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![None; num_rows])
        };

        let instance_ids = get_string("instance_id");
        let repos = get_string("repo");
        let base_commits = get_string("base_commit");
        let patches = get_string("patch");
        let test_patches = get_string("test_patch");
        let problem_statements = get_string("problem_statement");
        let hints = get_string("hints_text");
        let created_ats = get_string("created_at");
        let fail_to_passes = get_string("FAIL_TO_PASS");
        let pass_to_passes = get_string("PASS_TO_PASS");
        let languages = get_string("language");
        let difficulties = get_string("difficulty");

        let difficulty_scores: Vec<u8> = batch
            .column_by_name("difficulty_score")
            .and_then(|col| col.as_any().downcast_ref::<arrow::array::UInt8Array>())
            .map(|arr| {
                (0..num_rows)
                    .map(|i| if arr.is_null(i) { 1 } else { arr.value(i) })
                    .collect()
            })
            .unwrap_or_else(|| vec![1; num_rows]);

        let quality_scores: Vec<Option<f64>> = batch
            .column_by_name("quality_score")
            .and_then(|col| col.as_any().downcast_ref::<arrow::array::Float64Array>())
            .map(|arr| {
                (0..num_rows)
                    .map(|i| {
                        if arr.is_null(i) {
                            None
                        } else {
                            Some(arr.value(i))
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![None; num_rows]);

        for i in 0..num_rows {
            let id = instance_ids[i].clone().unwrap_or_default();
            let repo = repos[i].clone().unwrap_or_default();
            if id.is_empty() || repo.is_empty() {
                continue;
            }

            let f2p_str = fail_to_passes[i]
                .clone()
                .unwrap_or_else(|| "[]".to_string());
            let p2p_str = pass_to_passes[i]
                .clone()
                .unwrap_or_else(|| "[]".to_string());
            let fail_to_pass: Vec<String> = serde_json::from_str(&f2p_str).unwrap_or_default();
            let pass_to_pass: Vec<String> = serde_json::from_str(&p2p_str).unwrap_or_default();

            let created_at = created_ats[i]
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);

            let mut task = SweTask::new(&id, &repo);
            task.base_commit = base_commits[i].clone().unwrap_or_default();
            task.patch = patches[i].clone().unwrap_or_default();
            task.test_patch = test_patches[i].clone().unwrap_or_default();
            task.prompt = problem_statements[i].clone().unwrap_or_default();
            task.original_pr_body = hints[i].clone().unwrap_or_default();
            task.language = languages[i]
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            task.difficulty_score = difficulty_scores[i];
            task.quality_score = quality_scores[i];
            task.quality_passed = true;
            task.fail_to_pass = fail_to_pass;
            task.pass_to_pass = pass_to_pass;
            task.created_at = created_at;
            task.status = crate::swe::SweTaskStatus::Exported;

            if let Some(ref d) = difficulties[i] {
                task.meta.insert("difficulty".to_string(), d.clone());
            }

            tasks.push(task);
        }
    }

    tracing::info!(
        path = %input_path.display(),
        rows = tasks.len(),
        "Parquet file loaded"
    );

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swe::SweTask;

    fn make_test_task(id: &str) -> SweTask {
        let mut task = SweTask::new(id, "test-org/test-repo");
        task.base_commit = "abc123def456".to_string();
        task.patch = "diff --git a/file.py\n+fixed\n".to_string();
        task.prompt = "Fix the bug in module X".to_string();
        task.language = "python".to_string();
        task.difficulty_score = 2;
        task.quality_score = Some(0.75);
        task.quality_passed = true;
        task.fail_to_pass = vec!["pytest tests/test_x.py::test_fix".to_string()];
        task.pass_to_pass = vec!["pytest tests/test_x.py::test_other".to_string()];
        task.meta
            .insert("difficulty".to_string(), "medium".to_string());
        task
    }

    #[test]
    fn test_schema_fields() {
        let schema = swe_bench_schema();
        assert!(schema.field_with_name("instance_id").is_ok());
        assert!(schema.field_with_name("repo").is_ok());
        assert!(schema.field_with_name("FAIL_TO_PASS").is_ok());
        assert!(schema.field_with_name("PASS_TO_PASS").is_ok());
        assert!(schema.field_with_name("difficulty").is_ok());
        assert!(schema.field_with_name("quality_score").is_ok());
        assert_eq!(schema.fields().len(), 16);
    }

    #[test]
    fn test_tasks_to_record_batch() {
        let tasks = vec![make_test_task("task-001"), make_test_task("task-002")];
        let batch = tasks_to_record_batch(&tasks).unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 16);
    }

    #[test]
    fn test_write_and_read_parquet() {
        let tasks = vec![make_test_task("task-round-trip")];
        let tmp = std::env::temp_dir().join("swe_forge_test_parquet.parquet");

        write_parquet(&tasks, &tmp).unwrap();
        assert!(tmp.exists());

        let loaded = read_parquet(&tmp).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "task-round-trip");
        assert_eq!(loaded[0].repo, "test-org/test-repo");
        assert_eq!(loaded[0].language, "python");
        assert_eq!(loaded[0].difficulty_score, 2);
        assert_eq!(loaded[0].fail_to_pass.len(), 1);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_write_parquet_bytes() {
        let tasks = vec![make_test_task("task-bytes")];
        let bytes = write_parquet_bytes(&tasks).unwrap();
        assert!(!bytes.is_empty());
        // Parquet magic bytes: PAR1
        assert_eq!(&bytes[..4], b"PAR1");
    }

    #[test]
    fn test_empty_tasks_error() {
        let tasks: Vec<SweTask> = vec![];
        let tmp = std::env::temp_dir().join("swe_forge_empty.parquet");
        assert!(write_parquet(&tasks, &tmp).is_err());
    }
}
