use std::fmt;

use dyn_clone::DynClone;

#[allow(unused_variables)]
pub trait MarkdownVisitor: DynClone + fmt::Debug {
    fn visit_code(&self, code: &str) -> Option<String> {
        None
    }

    fn visit_custom_block(&self, name: &str, content: &str) -> Option<String> {
        None
    }
}
