//! What a query fetches: `&T`, `&mut T`, and tuples of them (up to 4).

use std::any::TypeId;

use crate::component::Component;
use concordia_reflect::DynReflect;

use crate::storage::column::{Column, typed, typed_mut};

/// A column (or shared value) as a query fetches it (implementation detail of [`QueryElem`]).
#[doc(hidden)]
pub struct ViewColumn<'a>(pub(crate) ViewCol<'a>);

/// Shared for reads, exclusive for writes; a shared or tag component is one value for all rows.
pub(crate) enum ViewCol<'a> {
    /// Read-only.
    Read(&'a dyn Column),
    /// Writable.
    Write(&'a mut dyn Column),
    /// One value for every row (shared and tag components; read-only).
    Shared(&'a dyn DynReflect),
}

/// The rows a `&T` element reads: a column, or one shared value for every row.
#[doc(hidden)]
pub enum ReadSlice<'a, T> {
    /// One value per row.
    Rows(&'a [T]),
    /// The same value for every row.
    One(&'a T),
}

/// Which component an element touches, and how.
#[derive(Debug, Clone, Copy)]
pub struct ElemAccess {
    /// The component's Rust type.
    pub type_id: TypeId,
    /// Its stable name.
    pub name: &'static str,
    /// True for `&mut T`.
    pub mutable: bool,
}

/// One component access in a query: `&T` or `&mut T`.
pub trait QueryElem {
    /// Per-entity item.
    type Item<'a>;
    /// Per-table slice.
    type Slice<'a>: Send;
    /// The access this element needs.
    fn access() -> ElemAccess;
    #[doc(hidden)]
    fn slice_view<'a>(column: ViewColumn<'a>) -> Self::Slice<'a>;
    #[doc(hidden)]
    fn get<'s>(slice: &'s mut Self::Slice<'_>, row: usize) -> Self::Item<'s>;
    #[doc(hidden)]
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>);
}

impl<T: Component> QueryElem for &T {
    type Item<'a> = &'a T;
    type Slice<'a> = ReadSlice<'a, T>;

    fn access() -> ElemAccess {
        ElemAccess {
            type_id: TypeId::of::<T>(),
            name: T::type_info().name,
            mutable: false,
        }
    }
    fn slice_view<'a>(column: ViewColumn<'a>) -> ReadSlice<'a, T> {
        match column.0 {
            ViewCol::Read(c) => ReadSlice::Rows(typed::<T>(c)),
            ViewCol::Write(c) => ReadSlice::Rows(typed::<T>(c)),
            ViewCol::Shared(v) => match v.as_any().downcast_ref::<T>() {
                Some(one) => ReadSlice::One(one),
                None => {
                    debug_assert!(false, "a shared value of another type");
                    ReadSlice::Rows(&[])
                }
            },
        }
    }
    fn get<'s>(slice: &'s mut ReadSlice<'_, T>, row: usize) -> &'s T {
        match slice {
            ReadSlice::Rows(rows) => &rows[row],
            ReadSlice::One(one) => one,
        }
    }
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
        match slice {
            ReadSlice::Rows(rows) => {
                let (a, b) = rows.split_at(mid);
                (ReadSlice::Rows(a), ReadSlice::Rows(b))
            }
            ReadSlice::One(one) => (ReadSlice::One(one), ReadSlice::One(one)),
        }
    }
}

impl<T: Component> QueryElem for &mut T {
    type Item<'a> = &'a mut T;
    type Slice<'a> = &'a mut [T];

    fn access() -> ElemAccess {
        ElemAccess {
            type_id: TypeId::of::<T>(),
            name: T::type_info().name,
            mutable: true,
        }
    }
    fn slice_view<'a>(column: ViewColumn<'a>) -> &'a mut [T] {
        match column.0 {
            ViewCol::Write(c) => typed_mut::<T>(c),
            // Writes are checked against declarations and storage kinds before fetching.
            ViewCol::Read(_) | ViewCol::Shared(_) => {
                debug_assert!(false, "a read-only column fetched for writing");
                &mut []
            }
        }
    }
    fn get<'s>(slice: &'s mut &mut [T], row: usize) -> &'s mut T {
        &mut slice[row]
    }
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
        slice.split_at_mut(mid)
    }
}

/// The data a query yields per entity.
pub trait QueryData {
    /// Per-entity item.
    type Item<'a>;
    /// Per-table slices.
    type Slices<'a>: Send;
    /// Accesses, one per element, in order.
    fn elems() -> Vec<ElemAccess>;
    #[doc(hidden)]
    fn fetch_view<'a>(columns: Vec<ViewColumn<'a>>) -> Self::Slices<'a>;
    #[doc(hidden)]
    fn get<'s>(slices: &'s mut Self::Slices<'_>, row: usize) -> Self::Item<'s>;
    #[doc(hidden)]
    fn split_at<'a>(slices: Self::Slices<'a>, mid: usize) -> (Self::Slices<'a>, Self::Slices<'a>);
}

macro_rules! single {
    ($ty:ty) => {
        impl<'r, T: Component> QueryData for $ty {
            type Item<'a> = <$ty as QueryElem>::Item<'a>;
            type Slices<'a> = <$ty as QueryElem>::Slice<'a>;

            fn elems() -> Vec<ElemAccess> {
                vec![<$ty as QueryElem>::access()]
            }
            fn fetch_view<'a>(columns: Vec<ViewColumn<'a>>) -> Self::Slices<'a> {
                let column = columns.into_iter().next().expect("one column");
                <$ty as QueryElem>::slice_view(column)
            }
            fn get<'s>(slices: &'s mut Self::Slices<'_>, row: usize) -> Self::Item<'s> {
                <$ty as QueryElem>::get(slices, row)
            }
            fn split_at<'a>(
                slices: Self::Slices<'a>,
                mid: usize,
            ) -> (Self::Slices<'a>, Self::Slices<'a>) {
                <$ty as QueryElem>::split_at(slices, mid)
            }
        }
    };
}

single!(&'r T);
single!(&'r mut T);

macro_rules! tuple {
    ($n:literal: $($e:ident $v:ident),+) => {
        impl<$($e: QueryElem),+> QueryData for ($($e,)+) {
            type Item<'a> = ($($e::Item<'a>,)+);
            type Slices<'a> = ($($e::Slice<'a>,)+);

            fn elems() -> Vec<ElemAccess> {
                vec![$($e::access()),+]
            }
            fn fetch_view<'a>(columns: Vec<ViewColumn<'a>>) -> Self::Slices<'a> {
                let mut columns = columns.into_iter();
                ($($e::slice_view(columns.next().expect("one column per element")),)+)
            }
            fn get<'s>(slices: &'s mut Self::Slices<'_>, row: usize) -> Self::Item<'s> {
                let ($($v,)+) = slices;
                ($($e::get($v, row),)+)
            }
            fn split_at<'a>(slices: Self::Slices<'a>, mid: usize) -> (Self::Slices<'a>, Self::Slices<'a>) {
                let ($($v,)+) = slices;
                $(let $v = $e::split_at($v, mid);)+
                (($($v.0,)+), ($($v.1,)+))
            }
        }
    };
}

tuple!(1: A a);
tuple!(2: A a, B b);
tuple!(3: A a, B b, C c);
tuple!(4: A a, B b, C c, D d);
