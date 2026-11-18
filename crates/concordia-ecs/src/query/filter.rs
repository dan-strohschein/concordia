//! Query filters: `With<T>`, `Without<T>`, `()` and tuples of filters.

use std::any::TypeId;
use std::marker::PhantomData;

use crate::component::Component;

/// Restricts which archetypes a query matches.
pub trait QueryFilter {
    /// Adds required and excluded component types.
    fn collect(with: &mut Vec<TypeId>, without: &mut Vec<TypeId>);
}

/// Matches entities that have `T` (without fetching it).
pub struct With<T>(PhantomData<T>);

/// Matches entities that lack `T`.
pub struct Without<T>(PhantomData<T>);

impl<T: Component> QueryFilter for With<T> {
    fn collect(with: &mut Vec<TypeId>, _: &mut Vec<TypeId>) {
        with.push(TypeId::of::<T>());
    }
}

impl<T: Component> QueryFilter for Without<T> {
    fn collect(_: &mut Vec<TypeId>, without: &mut Vec<TypeId>) {
        without.push(TypeId::of::<T>());
    }
}

impl QueryFilter for () {
    fn collect(_: &mut Vec<TypeId>, _: &mut Vec<TypeId>) {}
}

macro_rules! tuple {
    ($($f:ident),+) => {
        impl<$($f: QueryFilter),+> QueryFilter for ($($f,)+) {
            fn collect(with: &mut Vec<TypeId>, without: &mut Vec<TypeId>) {
                $($f::collect(with, without);)+
            }
        }
    };
}

tuple!(A, B);
tuple!(A, B, C);
