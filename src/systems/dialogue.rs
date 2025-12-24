//! Dialogue system functions.
//!
//! This module handles dialogue tree navigation and state management.
//! Functions here operate on Dialogue components directly (pure ECS pattern).

use crate::components::{Dialogue, DialogueNode};

/// Get the current dialogue node
pub fn current_node(dialogue: &Dialogue) -> Option<&DialogueNode> {
    dialogue.nodes.get(dialogue.current_node)
}

/// Advance to the next node based on option selection.
/// Returns true if dialogue continues, false if it ended.
pub fn select_option(dialogue: &mut Dialogue, option_index: usize) -> bool {
    if let Some(node) = dialogue.nodes.get(dialogue.current_node) {
        if let Some(option) = node.options.get(option_index) {
            if let Some(next) = option.next_node {
                dialogue.current_node = next;
                return true;
            }
        }
    }
    false
}

/// Reset dialogue to start
pub fn reset_dialogue(dialogue: &mut Dialogue) {
    dialogue.current_node = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::DialogueOption;

    fn create_test_dialogue() -> Dialogue {
        Dialogue {
            name: "Test NPC".to_string(),
            nodes: vec![
                DialogueNode {
                    text: "Hello!".to_string(),
                    options: vec![
                        DialogueOption {
                            label: "Hi".to_string(),
                            next_node: Some(1),
                        },
                        DialogueOption {
                            label: "Bye".to_string(),
                            next_node: None,
                        },
                    ],
                },
                DialogueNode {
                    text: "How are you?".to_string(),
                    options: vec![DialogueOption {
                        label: "Fine".to_string(),
                        next_node: None,
                    }],
                },
            ],
            current_node: 0,
        }
    }

    #[test]
    fn test_current_node() {
        let dialogue = create_test_dialogue();
        let node = current_node(&dialogue).unwrap();
        assert_eq!(node.text, "Hello!");
    }

    #[test]
    fn test_select_option_advances() {
        let mut dialogue = create_test_dialogue();
        assert!(select_option(&mut dialogue, 0)); // Select "Hi"
        assert_eq!(dialogue.current_node, 1);
    }

    #[test]
    fn test_select_option_ends() {
        let mut dialogue = create_test_dialogue();
        assert!(!select_option(&mut dialogue, 1)); // Select "Bye" (ends dialogue)
    }

    #[test]
    fn test_reset_dialogue() {
        let mut dialogue = create_test_dialogue();
        dialogue.current_node = 1;
        reset_dialogue(&mut dialogue);
        assert_eq!(dialogue.current_node, 0);
    }
}
