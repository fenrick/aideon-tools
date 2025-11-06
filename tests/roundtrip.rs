use aideon_tools::aideon::tools::flatten::build_workbook;
use aideon_tools::aideon::tools::io::excel_read;
use aideon_tools::aideon::tools::io::excel_write;
use aideon_tools::aideon::tools::io::jsonld;
use aideon_tools::aideon::tools::io::rdf::{self, RdfFormat};
use aideon_tools::aideon::tools::sync;
use std::fs;
use tempfile::tempdir;

#[test]
fn jsonld_excel_roundtrip_preserves_nodes() {
    let json_source = serde_json::json!({
        "@graph": [
            {
                "@id": "https://example.com/people/1",
                "@type": ["https://schema.org/Person", "https://schema.org/Agent"],
                "https://schema.org/name": "Alice",
                "https://schema.org/age": 30,
                "https://schema.org/knows": [{"@id": "https://example.com/people/2"}],
                "https://schema.org/skills": ["rust", "excel"]
            },
            {
                "@id": "https://example.com/people/2",
                "@type": "https://schema.org/Person",
                "https://schema.org/name": "Bob"
            }
        ]
    });

    let nodes = jsonld::parse_jsonld_document(&json_source).expect("JSON-LD parsed");
    let workbook = build_workbook(&nodes).expect("workbook built");
    let temp_dir = tempdir().expect("temporary directory");
    let xlsx_path = temp_dir.path().join("graph.xlsx");
    excel_write::write_workbook(&xlsx_path, &workbook).expect("Excel written");
    let restored_nodes = excel_read::read_nodes(&xlsx_path).expect("Excel read");

    assert_eq!(nodes, restored_nodes);
}

#[test]
fn rdf_roundtrip_matches_nodes() {
    let json_source = serde_json::json!({
        "@graph": [
            {
                "@id": "https://example.com/people/1",
                "@type": "https://schema.org/Person",
                "https://schema.org/name": "Alice",
                "https://schema.org/knows": {"@id": "https://example.com/people/2"}
            },
            {
                "@id": "https://example.com/people/2",
                "@type": "https://schema.org/Person",
                "https://schema.org/name": "Bob"
            }
        ]
    });

    let nodes = jsonld::parse_jsonld_document(&json_source).expect("JSON-LD parsed");
    let temp_dir = tempdir().expect("temporary directory");
    let rdf_path = temp_dir.path().join("graph.ttl");

    rdf::write_rdf(&rdf_path, &nodes, RdfFormat::Turtle).expect("RDF written");
    let restored_nodes = rdf::read_rdf(&rdf_path, Some(RdfFormat::Turtle)).expect("RDF read");

    assert_eq!(nodes, restored_nodes);
}

#[test]
fn excel_to_jsonld_includes_context() {
    let json_source = serde_json::json!({
        "@graph": [
            {
                "@id": "https://example.com/things/1",
                "@type": "https://schema.org/Thing",
                "https://schema.org/name": "Widget",
                "https://schema.org/category": ["tools", "widgets"],
                "https://schema.org/producer": {"@id": "https://example.com/orgs/1"}
            },
            {
                "@id": "https://example.com/orgs/1",
                "@type": "https://schema.org/Organization",
                "https://schema.org/name": "Acme"
            }
        ]
    });

    let nodes = jsonld::parse_jsonld_document(&json_source).expect("JSON-LD parsed");
    let workbook = build_workbook(&nodes).expect("workbook built");
    let temp_dir = tempdir().expect("temporary directory");
    let xlsx_path = temp_dir.path().join("graph.xlsx");
    excel_write::write_workbook(&xlsx_path, &workbook).expect("Excel written");

    let output_path = temp_dir.path().join("output.jsonld");
    let context = serde_json::json!({
        "@vocab": "https://schema.org/",
        "name": "https://schema.org/name",
        "category": "https://schema.org/category"
    });

    sync::excel_to_jsonld(&xlsx_path, &output_path, Some(context.clone()))
        .expect("Excel to JSON-LD conversion");

    let written = fs::read_to_string(&output_path).expect("JSON-LD file read");
    let parsed: serde_json::Value = serde_json::from_str(&written).expect("JSON parsed");

    assert_eq!(parsed.get("@context"), Some(&context));

    let graph = parsed
        .get("@graph")
        .and_then(|value| value.as_array())
        .expect("graph array");
    let thing = graph
        .iter()
        .find(|entry| {
            entry.get("@id")
                == Some(&serde_json::Value::String(
                    "https://example.com/things/1".into(),
                ))
        })
        .expect("thing node present");
    assert_eq!(
        thing.get("name"),
        Some(&serde_json::Value::String("Widget".into()))
    );
}

#[test]
fn jsonld_rdf_jsonld_roundtrip_preserves_nodes() {
    let json_source = serde_json::json!({
        "@context": {
            "@vocab": "https://schema.org/",
            "knows": {
                "@id": "https://schema.org/knows",
                "@type": "@id"
            }
        },
        "@graph": [
            {
                "@id": "https://example.com/people/1",
                "@type": "https://schema.org/Person",
                "https://schema.org/name": "Alice",
                "https://schema.org/knows": [
                    {"@id": "https://example.com/people/2"},
                    {"@id": "https://example.com/people/3"}
                ]
            },
            {
                "@id": "https://example.com/people/2",
                "@type": "https://schema.org/Person",
                "https://schema.org/name": "Bob"
            },
            {
                "@id": "https://example.com/people/3",
                "@type": "https://schema.org/Person",
                "https://schema.org/name": "Carol"
            }
        ]
    });

    let context = json_source
        .get("@context")
        .cloned()
        .expect("context available");

    let temp_dir = tempdir().expect("temporary directory");
    let json_path = temp_dir.path().join("input.jsonld");
    fs::write(
        &json_path,
        serde_json::to_string_pretty(&json_source).unwrap(),
    )
    .expect("JSON-LD input written");

    let rdf_path = temp_dir.path().join("graph.ttl");
    sync::jsonld_to_rdf(&json_path, &rdf_path, RdfFormat::Turtle).expect("JSON-LD to RDF");

    let roundtrip_path = temp_dir.path().join("roundtrip.jsonld");
    sync::rdf_to_jsonld(&rdf_path, &roundtrip_path, Some(context.clone())).expect("RDF to JSON-LD");

    let original_nodes =
        jsonld::parse_jsonld_document(&json_source).expect("original nodes parsed");

    let verification_rdf = temp_dir.path().join("verify.ttl");
    sync::jsonld_to_rdf(&roundtrip_path, &verification_rdf, RdfFormat::Turtle)
        .expect("roundtrip JSON-LD to RDF");

    let restored_nodes =
        rdf::read_rdf(&verification_rdf, Some(RdfFormat::Turtle)).expect("roundtrip nodes parsed");

    assert_eq!(original_nodes, restored_nodes);
}

#[test]
fn jsonld_excel_named_graph_roundtrip() {
    let json_source = serde_json::json!({
        "@graph": [
            {
                "@id": "https://example.com/resources/1",
                "http://schema.org/name": "Default"
            },
            {
                "@id": "https://example.com/graphs/named",
                "@graph": [
                    {
                        "@id": "https://example.com/resources/2",
                        "http://schema.org/name": "Named"
                    }
                ]
            }
        ]
    });

    let nodes = jsonld::parse_jsonld_document(&json_source).expect("JSON-LD parsed");
    let workbook = build_workbook(&nodes).expect("workbook built");
    let temp_dir = tempdir().expect("temporary directory");
    let xlsx_path = temp_dir.path().join("dataset.xlsx");
    excel_write::write_workbook(&xlsx_path, &workbook).expect("Excel written");
    let restored_nodes = excel_read::read_nodes(&xlsx_path).expect("Excel read");

    assert_eq!(nodes, restored_nodes);
    assert!(
        restored_nodes
            .iter()
            .any(|node| node.graph.as_deref() == Some("https://example.com/graphs/named"))
    );
}

#[test]
fn rdf_named_graph_roundtrip_matches_nodes() {
    let json_source = serde_json::json!({
        "@graph": [
            {
                "@id": "https://example.com/resources/1",
                "http://schema.org/name": "Default"
            },
            {
                "@id": "https://example.com/graphs/named",
                "@graph": [
                    {
                        "@id": "https://example.com/resources/2",
                        "http://schema.org/name": "Named"
                    }
                ]
            }
        ]
    });

    let nodes = jsonld::parse_jsonld_document(&json_source).expect("JSON-LD parsed");
    let temp_dir = tempdir().expect("temporary directory");
    let rdf_path = temp_dir.path().join("dataset.trig");

    rdf::write_rdf(&rdf_path, &nodes, RdfFormat::TriG).expect("RDF written");
    let restored_nodes = rdf::read_rdf(&rdf_path, Some(RdfFormat::TriG)).expect("RDF read");

    assert_eq!(nodes, restored_nodes);
}
