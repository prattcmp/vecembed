import os
import re

def extract_embeddable_entities(file_path):
    with open(file_path, 'r') as file:
        content = file.read()
        # Adjust the regex to capture only the module part before '::Entity'
        matches = re.findall(r'embeddable_entity!\(\s*(\w+)::Entity\s*,', content)
        return list(set(matches))

collections_file_path = 'src/embed/collections.rs'  # Update this path if needed
embeddable_entities = extract_embeddable_entities(collections_file_path)

# Generating a function with match statements for dynamic calls
dynamic_call_function = '''
pub async fn dynamic_import_embeddings(entity_name: &str) -> Result<(), crate::embed::import::ImportEmbeddingsError> {
    match entity_name {
'''

for entity in embeddable_entities:
        dynamic_call_function += f'        "{entity}" => crate::embed::import::import_embeddings::<super::{entity}::Entity, super::{entity}::Column>().await,\n'

dynamic_call_function += '''
        other => Err(crate::embed::import::ImportEmbeddingsError::UnknownCombination(other.to_string())),
    }
}
'''

# Write to a Rust file
with open('src/entities/string_convert.rs', 'w') as file:
    file.write(dynamic_call_function)

print("Code generated in string_convert.rs")
