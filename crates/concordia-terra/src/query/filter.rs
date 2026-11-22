//! Query filters: `With<T>`, `Without<T>`, `()` and tuples of filters.

use std::any::TypeId;
use std::marker::PhantomData;

use crate::component::Component;

/// Restricts which archetypes a query matches.
pub trait QueryFilter {
    /// Adds required and excluded component types.
    fn collect(with: &mut Vec<TypeId>, without: &mut Vec<TypeId>);

    /// Adds required and excluded components by stable name (for processor views).
    fn names(with: &mut Vec<&'static str>, without: &mut Vec<&'static str>);
}

fn name_of<T: Component>() -> &'static str {
    T::type_info().name
}

/// Matches entities that have `T` (without fetching it).
pub struct With<T>(PhantomData<T>);

/// Matches entities that lack `T`.
pub struct Without<T>(PhantomData<T>);

impl<T: Component> QueryFilter for With<T> {
    fn collect(with: &mut Vec<TypeId>, _: &mut Vec<TypeId>) {
        with.push(TypeId::of::<T>());
    }

    fn names(with: &mut Vec<&'static str>, _: &mut Vec<&'static str>) {
        with.push(name_of::<T>());
    }
}

impl<T: Component> QueryFilter for Without<T> {
    fn collect(_: &mut Vec<TypeId>, without: &mut Vec<TypeId>) {
        without.push(TypeId::of::<T>());
    }

    fn names(_: &mut Vec<&'static str>, without: &mut Vec<&'static str>) {
        without.push(name_of::<T>());
    }
}

impl QueryFilter for () {
    fn collect(_: &mut Vec<TypeId>, _: &mut Vec<TypeId>) {}

    fn names(_: &mut Vec<&'static str>, _: &mut Vec<&'static str>) {}
}

macro_rules! tuple {
    ($($f:ident),+) => {
        impl<$($f: QueryFilter),+> QueryFilter for ($($f,)+) {
            fn collect(with: &mut Vec<TypeId>, without: &mut Vec<TypeId>) {
                $($f::collect(with, without);)+
            }

            fn names(with: &mut Vec<&'static str>, without: &mut Vec<&'static str>) {
                $($f::names(with, without);)+
            }
        }
    };
}

tuple!(A, B);
tuple!(A, B, C);
