//! Read and write Parquet files (Arrow RecordBatch).

use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use rustafari_core::{Result, RustafariError};
use std::fs::File;
use std::path::Path;

/// Read a Parquet file into Arrow RecordBatches.
pub fn read_parquet(path: impl AsRef<Path>) -> Result<Vec<RecordBatch>> {
    let file = File::open(path.as_ref()).map_err(RustafariError::Io)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .map_err(|e| RustafariError::Lake(e.to_string()))?;
    let reader = builder.build().map_err(|e| RustafariError::Lake(e.to_string()))?;
    let batches: Vec<RecordBatch> = reader
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| RustafariError::Lake(e.to_string()))?;
    Ok(batches)
}

/// Write Arrow RecordBatches to a Parquet file.
pub fn write_parquet(path: impl AsRef<Path>, batches: &[RecordBatch]) -> Result<()> {
    if batches.is_empty() {
        return Ok(());
    }
    let file = File::create(path.as_ref()).map_err(RustafariError::Io)?;
    let schema = batches[0].schema().clone();
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))
        .map_err(|e| RustafariError::Lake(e.to_string()))?;
    for batch in batches {
        writer.write(batch).map_err(|e| RustafariError::Lake(e.to_string()))?;
    }
    writer.close().map_err(|e| RustafariError::Lake(e.to_string()))?;
    Ok(())
}
