//! # (De)serializing Arrows IPC format.
//!
//! Arrow IPC is a [binary format](https://arrow.apache.org/docs/python/ipc.html).
//! It is the recommended way to serialize and deserialize Polars DataFrames as this is most true
//! to the data schema.
//!
//! ## Example
//!
//! ```rust
//! use polars_core::prelude::*;
//! use polars_io::prelude::*;
//! use std::io::Cursor;
//!
//!
//! let s0 = Column::new("days".into(), &[0, 1, 2, 3, 4]);
//! let s1 = Column::new("temp".into(), &[22.1, 19.9, 7., 2., 3.]);
//! let mut df = DataFrame::new(vec![s0, s1]).unwrap();
//!
//! // Create an in memory file handler.
//! // Vec<u8>: Read + Write
//! // Cursor<T>: Seek
//!
//! let mut buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
//!
//! // write to the in memory buffer
//! IpcWriter::new(&mut buf).finish(&mut df).expect("ipc writer");
//!
//! // reset the buffers index after writing to the beginning of the buffer
//! buf.set_position(0);
//!
//! // read the buffer into a DataFrame
//! let df_read = IpcReader::new(buf).finish().unwrap();
//! assert!(df.equals(&df_read));
//! ```
use std::io::{Read, Seek};
use std::path::PathBuf;

use arrow::datatypes::{ArrowSchemaRef, Metadata};
use arrow::io::ipc::read::{self, get_row_count};
use arrow::record_batch::RecordBatch;
use polars_core::prelude::*;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::RowIndex;
use crate::hive::materialize_hive_partitions;
use crate::mmap::MmapBytesReader;
use crate::predicates::PhysicalIoExpr;
use crate::prelude::*;
use crate::shared::{ArrowReader, finish_reader};

#[derive(Clone, Debug, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "dsl-schema", derive(schemars::JsonSchema))]
pub struct IpcScanOptions;

#[expect(clippy::derivable_impls)]
impl Default for IpcScanOptions {
    fn default() -> Self {
        Self {}
    }
}

/// Read Arrows IPC format into a DataFrame
///
/// # Example
/// ```
/// use polars_core::prelude::*;
/// use std::fs::File;
/// use polars_io::ipc::IpcReader;
/// use polars_io::SerReader;
///
/// fn example() -> PolarsResult<DataFrame> {
///     let file = File::open("file.ipc").expect("file not found");
///
///     IpcReader::new(file)
///         .finish()
/// }
/// ```
#[must_use]
pub struct IpcReader<R: MmapBytesReader> {
    /// File or Stream object
    pub(super) reader: R,
    /// Aggregates chunks afterwards to a single chunk.
    rechunk: bool,
    pub(super) n_rows: Option<usize>,
    pub(super) projection: Option<Vec<usize>>,
    pub(crate) columns: Option<Vec<String>>,
    hive_partition_columns: Option<Vec<Series>>,
    include_file_path: Option<(PlSmallStr, Arc<str>)>,
    pub(super) row_index: Option<RowIndex>,
    // Stores the as key semaphore to make sure we don't write to the memory mapped file.
    pub(super) memory_map: Option<PathBuf>,
    metadata: Option<read::FileMetadata>,
    schema: Option<ArrowSchemaRef>,
}

fn check_mmap_err(err: PolarsError) -> PolarsResult<()> {
    if let PolarsError::ComputeError(s) = &err {
        if s.as_ref() == "memory_map can only be done on uncompressed IPC files" {
            eprintln!(
                "Could not memory_map compressed IPC file, defaulting to normal read. \
                Toggle off 'memory_map' to silence this warning."
            );
            return Ok(());
        }
    }
    Err(err)
}

impl<R: MmapBytesReader> IpcReader<R> {
    fn get_metadata(&mut self) -> PolarsResult<&read::FileMetadata> {
        if self.metadata.is_none() {
            let metadata = read::read_file_metadata(&mut self.reader)?;
            self.schema = Some(metadata.schema.clone());
            self.metadata = Some(metadata);
        }
        Ok(self.metadata.as_ref().unwrap())
    }

    /// Get arrow schema of the Ipc File.
    pub fn schema(&mut self) -> PolarsResult<ArrowSchemaRef> {
        self.get_metadata()?;
        Ok(self.schema.as_ref().unwrap().clone())
    }

    /// Get schema-level custom metadata of the Ipc file
    pub fn custom_metadata(&mut self) -> PolarsResult<Option<Arc<Metadata>>> {
        self.get_metadata()?;
        Ok(self
            .metadata
            .as_ref()
            .and_then(|meta| meta.custom_schema_metadata.clone()))
    }

    /// Stop reading when `n` rows are read.
    pub fn with_n_rows(mut self, num_rows: Option<usize>) -> Self {
        self.n_rows = num_rows;
        self
    }

    /// Columns to select/ project
    pub fn with_columns(mut self, columns: Option<Vec<String>>) -> Self {
        self.columns = columns;
        self
    }

    pub fn with_hive_partition_columns(mut self, columns: Option<Vec<Series>>) -> Self {
        self.hive_partition_columns = columns;
        self
    }

    pub fn with_include_file_path(
        mut self,
        include_file_path: Option<(PlSmallStr, Arc<str>)>,
    ) -> Self {
        self.include_file_path = include_file_path;
        self
    }

    /// Add a row index column.
    pub fn with_row_index(mut self, row_index: Option<RowIndex>) -> Self {
        self.row_index = row_index;
        self
    }

    /// Set the reader's column projection. This counts from 0, meaning that
    /// `vec![0, 4]` would select the 1st and 5th column.
    pub fn with_projection(mut self, projection: Option<Vec<usize>>) -> Self {
        self.projection = projection;
        self
    }

    /// Set if the file is to be memory_mapped. Only works with uncompressed files.
    /// The file name must be passed to register the memory mapped file.
    pub fn memory_mapped(mut self, path_buf: Option<PathBuf>) -> Self {
        self.memory_map = path_buf;
        self
    }

    // todo! hoist to lazy crate
    #[cfg(feature = "lazy")]
    pub fn finish_with_scan_ops(
        mut self,
        predicate: Option<Arc<dyn PhysicalIoExpr>>,
        verbose: bool,
    ) -> PolarsResult<DataFrame> {
        if self.memory_map.is_some() && self.reader.to_file().is_some() {
            if verbose {
                eprintln!("memory map ipc file")
            }
            match self.finish_memmapped(predicate.clone()) {
                Ok(df) => return Ok(df),
                Err(err) => check_mmap_err(err)?,
            }
        }
        let rechunk = self.rechunk;
        let metadata = read::read_file_metadata(&mut self.reader)?;

        // NOTE: For some code paths this already happened. See
        // https://github.com/pola-rs/polars/pull/14984#discussion_r1520125000
        // where this was introduced.
        if let Some(columns) = &self.columns {
            self.projection = Some(columns_to_projection(columns, &metadata.schema)?);
        }

        let schema = if let Some(projection) = &self.projection {
            Arc::new(apply_projection(&metadata.schema, projection))
        } else {
            metadata.schema.clone()
        };

        let reader = read::FileReader::new(self.reader, metadata, self.projection, self.n_rows);

        finish_reader(reader, rechunk, None, predicate, &schema, self.row_index)
    }
}

impl<R: MmapBytesReader> ArrowReader for read::FileReader<R>
where
    R: Read + Seek,
{
    fn next_record_batch(&mut self) -> PolarsResult<Option<RecordBatch>> {
        self.next().map_or(Ok(None), |v| v.map(Some))
    }
}

impl<R: MmapBytesReader> SerReader<R> for IpcReader<R> {
    fn new(reader: R) -> Self {
        IpcReader {
            reader,
            rechunk: true,
            n_rows: None,
            columns: None,
            hive_partition_columns: None,
            include_file_path: None,
            projection: None,
            row_index: None,
            memory_map: None,
            metadata: None,
            schema: None,
        }
    }

    fn set_rechunk(mut self, rechunk: bool) -> Self {
        self.rechunk = rechunk;
        self
    }

    fn finish(mut self) -> PolarsResult<DataFrame> {
        let reader_schema = if let Some(ref schema) = self.schema {
            schema.clone()
        } else {
            self.get_metadata()?.schema.clone()
        };
        let reader_schema = reader_schema.as_ref();

        let hive_partition_columns = self.hive_partition_columns.take();
        let include_file_path = self.include_file_path.take();

        // In case only hive columns are projected, the df would be empty, but we need the row count
        // of the file in order to project the correct number of rows for the hive columns.
        let mut df = (|| {
            if self.projection.as_ref().is_some_and(|x| x.is_empty()) {
                let row_count = if let Some(v) = self.n_rows {
                    v
                } else {
                    get_row_count(&mut self.reader)? as usize
                };
                let mut df = DataFrame::empty_with_height(row_count);

                if let Some(ri) = &self.row_index {
                    unsafe { df.with_row_index_mut(ri.name.clone(), Some(ri.offset)) };
                }
                return PolarsResult::Ok(df);
            }

            if self.memory_map.is_some() && self.reader.to_file().is_some() {
                match self.finish_memmapped(None) {
                    Ok(df) => {
                        return Ok(df);
                    },
                    Err(err) => check_mmap_err(err)?,
                }
            }
            let rechunk = self.rechunk;
            let schema = self.get_metadata()?.schema.clone();

            if let Some(columns) = &self.columns {
                let prj = columns_to_projection(columns, schema.as_ref())?;
                self.projection = Some(prj);
            }

            let schema = if let Some(projection) = &self.projection {
                Arc::new(apply_projection(schema.as_ref(), projection))
            } else {
                schema
            };

            let metadata = self.get_metadata()?.clone();

            let ipc_reader =
                read::FileReader::new(self.reader, metadata, self.projection, self.n_rows);
            let df = finish_reader(ipc_reader, rechunk, None, None, &schema, self.row_index)?;
            Ok(df)
        })()?;

        if let Some(hive_cols) = hive_partition_columns {
            materialize_hive_partitions(&mut df, reader_schema, Some(hive_cols.as_slice()));
        };

        if let Some((col, value)) = include_file_path {
            unsafe {
                df.with_column_unchecked(Column::new_scalar(
                    col,
                    Scalar::new(
                        DataType::String,
                        AnyValue::StringOwned(value.as_ref().into()),
                    ),
                    df.height(),
                ))
            };
        }

        Ok(df)
    }
}
