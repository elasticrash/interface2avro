use serde_json::{json, Map, Value};
use std::env;
use std::fs::File;
use std::io::Read;
use tree_sitter::Parser;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];

    let code = match File::open(file_path) {
        Ok(mut file) => {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                contents
            } else {
                eprintln!("Failed to read the file.");
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("Error opening the file: {}", err);
            std::process::exit(1);
        }
    }
    .to_owned();

    let schemas = get_schema(code);
    let candidate_schema = merger(schemas);

    println!("{}", json!(candidate_schema));
}

fn merger(schemas: Vec<Value>) -> Value {
    let mut candidate_schema = schemas[0].clone();

    let base_types = vec!["string", "number", "null", "Date", "boolean"];

    for (i, entry) in schemas[0]["fields"].as_array().unwrap().iter().enumerate() {
        if base_types
            .iter()
            .find(|&x| {
                return **x == entry["type"];
            })
            .is_none()
        {
            let sub_schema = schemas.iter().find(|&x| x["name"] == entry["type"]);
            if sub_schema.is_some() {
                candidate_schema["fields"].as_array_mut().unwrap()[i] = sub_schema.unwrap().clone();
            }
        }
    }

    candidate_schema
}

fn get_schema(code: String) -> Vec<Value> {
    let mut vec_map = Vec::new();

    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_typescript::language_typescript())
        .expect("Error loading typescript grammar");
    let parsed = parser.parse(code.clone(), None).unwrap();
    let root = parsed.root_node();
    let mut root_iter = root.walk();
    for node in root_iter.node().children(&mut root_iter) {
        if node.kind() == "interface_declaration" {
            let mut map = Map::new();
            map.insert("type".to_owned(), Value::String("Record".to_owned()));
            let mut fields = Vec::new();
            let mut interface = node.walk();

            node.children(&mut interface).for_each(|node| {
                let iname = node.utf8_text(&code.as_bytes()).unwrap();

                match node.kind() {
                    "type_identifier" => {
                        map.insert("name".to_owned(), Value::String(iname.to_owned()));
                    }
                    "object_type" => {
                        let mut oter = node.walk();
                        node.children(&mut oter).for_each(|node| {
                            let prop = get_prop_type(&node, code.clone());

                            if prop.is_some() {
                                fields.push(prop.unwrap());
                            }
                        });
                    }
                    _ => {}
                }
            });

            map.insert("fields".to_owned(), Value::Array(fields));
            let json_value = json!(map);
            vec_map.push(json_value);
        }
    }

    vec_map
}

fn get_prop_type(c_node: &tree_sitter::Node, code: String) -> Option<Value> {
    let mut pptype: Option<Value> = None;
    let mut ppvalue: Option<String> = None;

    let mut cursor = c_node.walk();
    c_node.children(&mut cursor).for_each(|node| {
        let propd = node.utf8_text(&code.as_bytes()).unwrap();
        if propd.chars().collect::<Vec<char>>()[0] == ':' {
            let mut subtype = node.walk();
            node.children(&mut subtype).for_each(|node| {
                let typed = node.utf8_text(code.as_bytes()).unwrap().to_owned();
                if typed != ":" {
                    if typed.contains('|') {
                        let mut col = Vec::new();
                        typed.split('|').for_each(|c| {
                            col.push(Value::String(c.trim().to_owned()));
                        });
                        pptype = Some(Value::Array(col));
                    } else {
                        pptype = Some(Value::String(typed));
                    }
                }
            });
        } else {
            ppvalue = Some(propd.to_string());
        }
    });

    if ppvalue.is_some() && pptype.is_some() {
        return Some(json!({
            "name": ppvalue.unwrap(),
            "type": pptype.unwrap()
        }));
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::{get_schema, merger};

    #[test]
    fn test_basic_model() {
        let code = r#"
        interface Person {
            age: number;
            location: string | null;
        }
        "#;

        let schemas = get_schema(code.to_string());
        let schema = merger(schemas);

        assert_eq!(schema["type"], "Record");
        assert_eq!(schema["name"], "Person");
        assert_eq!(schema["fields"][0]["name"], "age");
        assert_eq!(schema["fields"][0]["type"], "number");
        assert_eq!(schema["fields"][1]["name"], "location");
        assert_eq!(schema["fields"][1]["type"][0], "string");
        assert_eq!(schema["fields"][1]["type"][1], "null");
    }

    #[test]
    fn test_nested_model() {
        let code = r#"
        interface Person {
            age: number;
            location: Location;
        }

        interface Location {
            city: string;
            state: string;
        }
        "#;

        let schemas = get_schema(code.to_string());
        let schema = merger(schemas);

        assert_eq!(schema["type"], "Record");
        assert_eq!(schema["name"], "Person");
        assert_eq!(schema["fields"][0]["name"], "age");
        assert_eq!(schema["fields"][0]["type"], "number");
        assert_eq!(schema["fields"][1]["name"], "Location");
        assert_eq!(schema["fields"][1]["fields"][0]["name"], "city");
        assert_eq!(schema["fields"][1]["fields"][0]["type"], "string");
        assert_eq!(schema["fields"][1]["fields"][1]["name"], "state");
        assert_eq!(schema["fields"][1]["fields"][1]["type"], "string");
    }
}
