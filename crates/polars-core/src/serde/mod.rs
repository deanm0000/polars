pub mod chunked_array;
mod df;
pub mod series;

#[cfg(test)]
mod test {
    use crate::chunked_array::flags::StatisticsFlags;
    use crate::prelude::*;
    use crate::series::IsSorted;

    #[test]
    fn test_serde() -> PolarsResult<()> {
        let ca = UInt32Chunked::new("foo".into(), &[Some(1), None, Some(2)]);

        let json = serde_json::to_string(&ca.clone().into_series()).unwrap();

        let out = serde_json::from_str::<Series>(&json).unwrap();
        assert!(ca.into_series().equals_missing(&out));

        let ca = StringChunked::new("foo".into(), &[Some("foo"), None, Some("bar")]);

        let json = serde_json::to_string(&ca.clone().into_series()).unwrap();

        let out = serde_json::from_str::<Series>(&json).unwrap(); // uses `Deserialize<'de>`
        assert!(ca.into_series().equals_missing(&out));

        Ok(())
    }

    /// test using the `DeserializedOwned` trait
    #[test]
    fn test_serde_owned() {
        let ca = UInt32Chunked::new("foo".into(), &[Some(1), None, Some(2)]);

        let json = serde_json::to_string(&ca.clone().into_series()).unwrap();

        let out = serde_json::from_reader::<_, Series>(json.as_bytes()).unwrap(); // uses `DeserializeOwned`
        assert!(ca.into_series().equals_missing(&out));
    }

    fn sample_dataframe() -> DataFrame {
        let s1 = Series::new("foo".into(), &[1, 2, 3]);
        let s2 = Series::new("bar".into(), &[Some(true), None, Some(false)]);
        let s3 = Series::new("string".into(), &["mouse", "elephant", "dog"]);
        let s_list = Column::new("list".into(), &[s1.clone(), s1.clone(), s1.clone()]);

        DataFrame::new(vec![s1.into(), s2.into(), s3.into(), s_list]).unwrap()
    }

    #[test]
    fn test_serde_flags() {
        let df = sample_dataframe();

        for mut column in df.columns {
            column.set_sorted_flag(IsSorted::Descending);
            let json = serde_json::to_string(&column).unwrap();
            let out = serde_json::from_reader::<_, Column>(json.as_bytes()).unwrap();
            let f = out.get_flags();
            assert_ne!(f, StatisticsFlags::empty());
            assert_eq!(column.get_flags(), out.get_flags());
        }
    }

    #[test]
    fn test_serde_df_json() {
        let df = sample_dataframe();
        let json = serde_json::to_string(&df).unwrap();
        let out = serde_json::from_str::<DataFrame>(&json).unwrap(); // uses `Deserialize<'de>`
        assert!(df.equals_missing(&out));
    }

    /// test using the `DeserializedOwned` trait
    #[test]
    fn test_serde_df_owned_json() {
        let df = sample_dataframe();
        let json = serde_json::to_string(&df).unwrap();

        let out = serde_json::from_reader::<_, DataFrame>(json.as_bytes()).unwrap(); // uses `DeserializeOwned`
        assert!(df.equals_missing(&out));
    }

    // STRUCT REFACTOR
    #[ignore]
    #[test]
    #[cfg(feature = "dtype-struct")]
    fn test_serde_struct_series_owned_json() {
        let row_1 = AnyValue::StructOwned(Box::new((
            vec![
                AnyValue::String("1:1"),
                AnyValue::Null,
                AnyValue::String("1:3"),
            ],
            vec![
                Field::new("fld_1".into(), DataType::String),
                Field::new("fld_2".into(), DataType::String),
                Field::new("fld_3".into(), DataType::String),
            ],
        )));
        let dtype = DataType::Struct(vec![
            Field::new("fld_1".into(), DataType::String),
            Field::new("fld_2".into(), DataType::String),
            Field::new("fld_3".into(), DataType::String),
        ]);
        let row_2 = AnyValue::StructOwned(Box::new((
            vec![
                AnyValue::String("2:1"),
                AnyValue::String("2:2"),
                AnyValue::String("2:3"),
            ],
            vec![
                Field::new("fld_1".into(), DataType::String),
                Field::new("fld_2".into(), DataType::String),
                Field::new("fld_3".into(), DataType::String),
            ],
        )));
        let row_3 = AnyValue::Null;

        let s =
            Series::from_any_values_and_dtype("item".into(), &[row_1, row_2, row_3], &dtype, false)
                .unwrap();
        let df = DataFrame::new(vec![s.into()]).unwrap();

        let df_str = serde_json::to_string(&df).unwrap();
        let out = serde_json::from_str::<DataFrame>(&df_str).unwrap();
        assert!(df.equals_missing(&out));
    }
}
