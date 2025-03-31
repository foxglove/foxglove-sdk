#[cfg(test)]
mod test {
    use crate::encode::Encode;
    use crate::schemas::GeoJson;
    #[test]
    fn test_geojson_schema_preserves_schema_name() {
        let schema = GeoJson::get_schema();
        assert!(schema.is_some());
        assert_eq!(schema.unwrap().name, "foxglove.GeoJSON");
    }
}
