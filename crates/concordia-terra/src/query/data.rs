//! What a query fetches: `&T`, `&mut T`, and tuples of them (up to 4).

use std::any::TypeId;

use crate::component::Component;
use crate::storage::column::{Column, typed, typed_mut};

/// Opaque access to one table's columns (implementation detail of [`QueryData`]).
#[doc(hidden)]
pub struct TableAccess<'a>(pub(crate) &'a mut [Box<dyn Column>]);

/// Opaque access to one column (implementation detail of [`QueryElem`]).
#[doc(hidden)]
pub struct ColumnAccess<'a>(&'a mut dyn Column);

/// A column as a processor's view holds it (implementation detail of [`QueryElem`]).
#[doc(hidden)]
pub struct ViewColumn<'a>(pub(crate) ViewCol<'a>);

/// Shared for reads, exclusive for writes.
pub(crate) enum ViewCol<'a> {
    /// Read-only.
    Read(&'a dyn Column),
    /// Writable.
    Write(&'a mut dyn Column),
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
    fn slice<'a>(column: ColumnAccess<'a>) -> Self::Slice<'a>;
    #[doc(hidden)]
    fn slice_view<'a>(column: ViewColumn<'a>) -> Self::Slice<'a>;
    #[doc(hidden)]
    fn get<'s>(slice: &'s mut Self::Slice<'_>, row: usize) -> Self::Item<'s>;
    #[doc(hidden)]
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>);
}

impl<T: Component> QueryElem for &T {
    type Item<'a> = &'a T;
    type Slice<'a> = &'a [T];

    fn access() -> ElemAccess {
        ElemAccess {
            type_id: TypeId::of::<T>(),
            name: T::type_info().name,
            mutable: false,
        }
    }
    fn slice<'a>(column: ColumnAccess<'a>) -> &'a [T] {
        typed::<T>(column.0)
    }
    fn slice_view<'a>(column: ViewColumn<'a>) -> &'a [T] {
        match column.0 {
            ViewCol::Read(c) => typed::<T>(c),
            ViewCol::Write(c) => typed::<T>(c),
        }
    }
    fn get<'s>(slice: &'s mut &[T], row: usize) -> &'s T {
        &slice[row]
    }
    fn split_at<'a>(slice: Self::Slice<'a>, mid: usize) -> (Self::Slice<'a>, Self::Slice<'a>) {
        slice.split_at(mid)
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
    fn slice<'a>(column: ColumnAccess<'a>) -> &'a mut [T] {
        typed_mut::<T>(column.0)
    }
    fn slice_view<'a>(column: ViewColumn<'a>) -> &'a mut [T] {
        match column.0 {
            ViewCol::Write(c) => typed_mut::<T>(c),
            // The view checks writes against the declaration before fetching.
            ViewCol::Read(_) => {
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
    fn fetch<'a>(table: TableAccess<'a>, columns: &[usize]) -> Self::Slices<'a>;
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
            fn fetch<'a>(table: TableAccess<'a>, columns: &[usize]) -> Self::Slices<'a> {
                let column = &mut *table.0[columns[0]];
                <$ty as QueryElem>::slice(ColumnAccess(column))
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
            fn fetch<'a>(table: TableAccess<'a>, columns: &[usize]) -> Self::Slices<'a> {
                let indices: [usize; $n] = columns.try_into().expect("one column per element");
                let [$($v),+] = table.0.get_disjoint_mut(indices).expect("query elements are distinct");
                ($($e::slice(ColumnAccess(&mut **$v)),)+)
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
