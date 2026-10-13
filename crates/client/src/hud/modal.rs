//! The single modal presentation owner for an online client.
//!
//! Script menus, client-owned shell screens and Create-a-Class used to live in
//! independent `Option`s. That allowed several to draw and claim different
//! parts of input at once. `Modal` makes those states mutually exclusive;
//! map-vote remains a non-modal gameplay overlay.

use super::{createaclass::CreateAClass, menu::MenuView, shell::Menu};

pub enum Modal {
    Shell(Menu),
    Script { name: String, view: MenuView },
    CreateAClass(CreateAClass),
}

pub enum ModalRef<'a> {
    Shell(&'a Menu),
    Script(&'a MenuView),
    CreateAClass(&'a CreateAClass),
}

impl Modal {
    pub fn as_ref(&self) -> ModalRef<'_> {
        match self {
            Modal::Shell(menu) => ModalRef::Shell(menu),
            Modal::Script { view, .. } => ModalRef::Script(view),
            Modal::CreateAClass(editor) => ModalRef::CreateAClass(editor),
        }
    }
}
