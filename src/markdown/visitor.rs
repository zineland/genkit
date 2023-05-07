use std::fmt;

use dyn_clone::DynClone;

use crate::code_blocks::Fenced;

#[allow(unused_variables)]
pub trait MarkdownVisitor: DynClone + fmt::Debug {
    fn visit_code(&self, code: &str) -> Option<String> {
        None
    }

    fn visit_custom_block(&self, fenced: Fenced, content: &str) -> Option<String> {
        None
    }
}
