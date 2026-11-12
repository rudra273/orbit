//! Embedding spaces: what makes two vectors comparable.

/// One model at one size, with the prefixes its model card prescribes. Vectors are only ever
/// compared within a space, and a space never changes once vectors exist in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Space {
    /// `<model>@<dims>`.
    pub id: String,
    pub model: String,
    pub dims: usize,
    pub query_prefix: String,
    pub doc_prefix: String,
    /// Text is sent off the server to embed; `sensitive` evidence is then never embedded.
    pub hosted: bool,
}

/// Retrieval instruction for Qwen3-Embedding, in the format from its model card. It is the one
/// measured on the owner's history (evals/embed-bench); documents take no prefix.
const QWEN3_QUERY_PREFIX: &str =
    "Instruct: Given a question or topic, retrieve the conversation turns that answer or discuss it\nQuery:";

impl Space {
    /// A space for a hosted model, with the prefixes known for its family.
    pub fn hosted(model: &str, dims: usize) -> Self {
        let query_prefix = if model.starts_with("qwen/qwen3-embedding") {
            QWEN3_QUERY_PREFIX
        } else {
            ""
        };
        Self {
            id: format!("{model}@{dims}"),
            model: model.to_string(),
            dims,
            query_prefix: query_prefix.to_string(),
            doc_prefix: String::new(),
            hosted: true,
        }
    }

    /// The text actually sent to the model.
    pub fn prepare(&self, role: crate::Role, text: &str) -> String {
        let prefix = match role {
            crate::Role::Query => &self.query_prefix,
            crate::Role::Document => &self.doc_prefix,
        };
        format!("{prefix}{text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Role;

    #[test]
    fn qwen_queries_carry_the_instruction_and_documents_do_not() {
        let space = Space::hosted("qwen/qwen3-embedding-8b", 4096);
        assert_eq!(space.id, "qwen/qwen3-embedding-8b@4096");
        assert!(space.prepare(Role::Query, "x").starts_with("Instruct: "));
        assert_eq!(space.prepare(Role::Document, "x"), "x");
    }
}
