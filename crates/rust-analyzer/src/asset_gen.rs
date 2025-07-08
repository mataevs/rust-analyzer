use crate::asset::{File, Node, Range, Annotation};
use syntax::{SourceFile, SyntaxNode, TextRange, Edition, WalkEvent};

/// Flattens the syntax tree into a list of Nodes (preorder traversal).
fn flatten_ast(node: &SyntaxNode) -> Vec<Node> {
    let mut nodes = Vec::new();
    for event in node.preorder_with_tokens() {
        if let WalkEvent::Enter(n) = event {
            if let Some(n) = n.as_node() {
                let kind = format!("{:?}", n.kind());
                let range = n.text_range();
                nodes.push(Node {
                    range: Range {
                        offset: range.start().into(),
                        end_offset: range.end().into(),
                    },
                    node_type: kind,
                });
            }
        }
    }
    nodes
}

/// Converts a TextRange to asset::Range.
fn range_from_text_range(r: TextRange) -> Range {
    Range {
        offset: r.start().into(),
        end_offset: r.end().into(),
    }
}

/// Parses a Rust file and produces an asset::File.
pub fn parse_rust_to_asset_file(path: String, content: String) -> File {
    let parse = SourceFile::parse(&content, Edition::CURRENT);
    let tree = flatten_ast(&parse.syntax_node());
    let errors = parse.errors().into_iter().map(|err| {
        Annotation {
            range: range_from_text_range(err.range()),
            text: err.to_string(),
        }
    }).collect();

    File {
        path,
        content,
        tree,
        errors,
    }
}