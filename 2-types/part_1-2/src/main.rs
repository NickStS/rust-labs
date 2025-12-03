mod part_1;
mod part_2;

use std::fs;

fn main() {
    println!("=== Part 2: JSON to TOML Conversion ===\n");
    
    let json_content = fs::read_to_string("../request.json")
        .expect("Failed to read request.json");
    
    let request = part_2::Request::from_json(&json_content)
        .expect("Failed to deserialize JSON");
    
    let toml_output = request.to_toml()
        .expect("Failed to serialize to TOML");
    
    println!("{}", toml_output);
}
