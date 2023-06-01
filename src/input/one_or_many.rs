use std::{mem, ops::Deref, slice, vec};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    pub fn as_slice(&self) -> &[T] {
        match self {
            OneOrMany::One(one) => slice::from_ref(one),
            OneOrMany::Many(many) => many,
        }
    }
}

impl<T> Deref for OneOrMany<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'a, T> IntoIterator for &'a OneOrMany<T> {
    type Item = &'a T;

    type IntoIter = slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<T> IntoIterator for OneOrMany<T> {
    type Item = T;

    type IntoIter = OneOrManyIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            OneOrMany::One(one) => OneOrManyIter::One(one),
            OneOrMany::Many(many) => OneOrManyIter::Many(many.into_iter()),
        }
    }
}

pub enum OneOrManyIter<T> {
    One(T),
    Many(vec::IntoIter<T>),
    Empty,
}

impl<T> Iterator for OneOrManyIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let mut current = OneOrManyIter::Empty;
        mem::swap(self, &mut current);
        match current {
            OneOrManyIter::One(one) => Some(one),
            OneOrManyIter::Many(mut many) => {
                let value = many.next();
                *self = OneOrManyIter::Many(many);
                value
            }
            OneOrManyIter::Empty => None,
        }
    }
}
