//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   Pierre Avital, <pierre.avital@me.com>
//

//! ABI-stable handles for opaque values.
//!
//! These types carry a logical opaque marker in their [`crate::IStable`] report
//! while exposing only a thin pointer at the ABI boundary.

use core::{marker::PhantomData, ptr::NonNull};

use crate::{report, str::Str, typenum2::*, IDeterminantProvider, IStable, StableLike};

/// An ABI-stable shared handle to an opaque value.
#[repr(transparent)]
pub struct Ref<Opaque: IStable> {
    ptr: NonNull<()>,
    marker: PhantomData<*const Opaque>,
}

impl<Opaque: IStable> Clone for Ref<Opaque> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Opaque: IStable> Copy for Ref<Opaque> {}

impl<Opaque: IStable> Ref<Opaque> {
    /// Creates an opaque shared handle from a raw non-null pointer.
    ///
    /// # Safety
    /// The pointer must refer to a value that the provider of `Opaque` accepts
    /// as the backing implementation for this handle, and it must remain valid
    /// for every call that receives the returned handle.
    pub const unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        Self {
            ptr,
            marker: PhantomData,
        }
    }

    /// Creates an opaque shared handle from a Rust reference.
    ///
    /// # Safety
    /// The referenced value must be the backing implementation expected by the
    /// provider of `Opaque` for every function that will consume the returned
    /// handle.
    pub unsafe fn from_ref<T>(value: &T) -> Self {
        Self {
            ptr: NonNull::from(value).cast(),
            marker: PhantomData,
        }
    }

    /// Returns the erased pointer carried by this handle.
    pub const fn as_ptr(self) -> NonNull<()> {
        self.ptr
    }

    /// Casts the handle back to a shared reference.
    ///
    /// # Safety
    /// `T` must be the actual backing implementation type for this handle.
    pub unsafe fn cast<T>(&self) -> &T {
        self.ptr.cast::<T>().as_ref()
    }
}

/// An ABI-stable shared opaque handle bound to a runtime-provided interface table.
#[repr(C)]
pub struct InterfaceRef<Opaque: IStable, VTable: IStable> {
    this: Ref<Opaque>,
    vtable: NonNull<VTable>,
}

impl<Opaque: IStable, VTable: IStable> Clone for InterfaceRef<Opaque, VTable> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Opaque: IStable, VTable: IStable> Copy for InterfaceRef<Opaque, VTable> {}

impl<Opaque: IStable, VTable: IStable> InterfaceRef<Opaque, VTable> {
    /// Creates a bound shared interface from an opaque handle and a vtable reference.
    pub fn new(this: Ref<Opaque>, vtable: &VTable) -> Self {
        Self {
            this,
            vtable: NonNull::from(vtable),
        }
    }

    /// Creates a bound shared interface from raw non-null pointers.
    ///
    /// # Safety
    /// `this` must be valid for the opaque API identified by `Opaque`, and
    /// `vtable` must point to a vtable that remains valid for every method call
    /// made through the returned handle.
    pub const unsafe fn from_raw(this: NonNull<()>, vtable: NonNull<VTable>) -> Self {
        Self {
            this: unsafe { Ref::from_raw(this) },
            vtable,
        }
    }

    /// Returns the underlying shared opaque handle.
    pub const fn as_opaque(&self) -> Ref<Opaque> {
        self.this
    }

    /// Returns the interface table pointer carried by this handle.
    pub const fn vtable_ptr(&self) -> NonNull<VTable> {
        self.vtable
    }

    /// Returns the interface table carried by this handle.
    pub fn vtable(&self) -> &VTable {
        // SAFETY: All constructors require a non-null vtable pointer that stays
        // valid while methods are called through this handle.
        unsafe { self.vtable.as_ref() }
    }

    /// Erases the vtable type from this bound interface.
    pub fn erase(self) -> ErasedInterfaceRef<Opaque> {
        ErasedInterfaceRef {
            this: self.this,
            vtable: self.vtable.cast(),
        }
    }
}

/// An ABI-stable shared opaque handle bound to an erased runtime-provided interface table.
#[repr(C)]
pub struct ErasedInterfaceRef<Opaque: IStable> {
    this: Ref<Opaque>,
    vtable: NonNull<()>,
}

impl<Opaque: IStable> Clone for ErasedInterfaceRef<Opaque> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Opaque: IStable> Copy for ErasedInterfaceRef<Opaque> {}

impl<Opaque: IStable> ErasedInterfaceRef<Opaque> {
    /// Creates an erased bound shared interface from an opaque handle and erased vtable pointer.
    pub const fn new(this: Ref<Opaque>, vtable: NonNull<()>) -> Self {
        Self { this, vtable }
    }

    /// Returns the underlying shared opaque handle.
    pub const fn as_opaque(&self) -> Ref<Opaque> {
        self.this
    }

    /// Returns the erased interface table pointer carried by this handle.
    pub const fn vtable_ptr(&self) -> NonNull<()> {
        self.vtable
    }

    /// Rebinds this erased interface to a concrete vtable type.
    ///
    /// # Safety
    /// The vtable pointer must point to a valid `VTable` that remains live for
    /// every method call through the returned handle. Callers should only use
    /// this after the provider has checked `VTable::REPORT` for compatibility.
    pub unsafe fn assume_vtable<VTable: IStable>(self) -> InterfaceRef<Opaque, VTable> {
        unsafe { InterfaceRef::from_raw(self.this.as_ptr(), self.vtable.cast()) }
    }
}

/// An ABI-stable exclusive handle to an opaque value.
#[repr(transparent)]
pub struct RefMut<Opaque: IStable> {
    ptr: NonNull<()>,
    marker: PhantomData<*mut Opaque>,
}

impl<Opaque: IStable> RefMut<Opaque> {
    /// Creates an opaque exclusive handle from a raw non-null pointer.
    ///
    /// # Safety
    /// The pointer must refer to a value that the provider of `Opaque` accepts
    /// as the backing implementation for this handle, and it must remain
    /// exclusively valid for every call that receives the returned handle.
    pub const unsafe fn from_raw(ptr: NonNull<()>) -> Self {
        Self {
            ptr,
            marker: PhantomData,
        }
    }

    /// Creates an opaque exclusive handle from a Rust mutable reference.
    ///
    /// # Safety
    /// The referenced value must be the backing implementation expected by the
    /// provider of `Opaque` for every function that will consume the returned
    /// handle.
    pub unsafe fn from_mut<T>(value: &mut T) -> Self {
        Self {
            ptr: NonNull::from(value).cast(),
            marker: PhantomData,
        }
    }

    /// Returns the erased pointer carried by this handle.
    pub const fn as_ptr(&self) -> NonNull<()> {
        self.ptr
    }

    /// Reborrows this handle as a shared opaque handle.
    pub fn as_ref(&self) -> Ref<Opaque> {
        Ref {
            ptr: self.ptr,
            marker: PhantomData,
        }
    }

    /// Reborrows this handle as an exclusive opaque handle.
    pub fn reborrow(&mut self) -> RefMut<Opaque> {
        RefMut {
            ptr: self.ptr,
            marker: PhantomData,
        }
    }

    /// Casts the handle back to a shared reference.
    ///
    /// # Safety
    /// `T` must be the actual backing implementation type for this handle.
    pub unsafe fn cast<T>(&self) -> &T {
        self.ptr.cast::<T>().as_ref()
    }

    /// Casts the handle back to an exclusive reference.
    ///
    /// # Safety
    /// `T` must be the actual backing implementation type for this handle.
    pub unsafe fn cast_mut<T>(&mut self) -> &mut T {
        self.ptr.cast::<T>().as_mut()
    }
}

/// An ABI-stable exclusive opaque handle bound to a runtime-provided interface table.
#[repr(C)]
pub struct InterfaceRefMut<Opaque: IStable, VTable: IStable> {
    this: RefMut<Opaque>,
    vtable: NonNull<VTable>,
}

impl<Opaque: IStable, VTable: IStable> InterfaceRefMut<Opaque, VTable> {
    /// Creates a bound exclusive interface from an opaque handle and a vtable reference.
    pub fn new(this: RefMut<Opaque>, vtable: &VTable) -> Self {
        Self {
            this,
            vtable: NonNull::from(vtable),
        }
    }

    /// Creates a bound exclusive interface from raw non-null pointers.
    ///
    /// # Safety
    /// `this` must be valid and exclusive for the opaque API identified by
    /// `Opaque`, and `vtable` must point to a vtable that remains valid for
    /// every method call made through the returned handle.
    pub const unsafe fn from_raw(this: NonNull<()>, vtable: NonNull<VTable>) -> Self {
        Self {
            this: unsafe { RefMut::from_raw(this) },
            vtable,
        }
    }

    /// Reborrows this bound handle as a shared bound handle.
    pub fn as_ref(&self) -> InterfaceRef<Opaque, VTable> {
        InterfaceRef {
            this: self.this.as_ref(),
            vtable: self.vtable,
        }
    }

    /// Reborrows this bound handle as an exclusive bound handle.
    pub fn reborrow(&mut self) -> InterfaceRefMut<Opaque, VTable> {
        InterfaceRefMut {
            this: self.this.reborrow(),
            vtable: self.vtable,
        }
    }

    /// Returns the underlying shared opaque handle.
    pub fn as_opaque(&self) -> Ref<Opaque> {
        self.this.as_ref()
    }

    /// Reborrows the underlying exclusive opaque handle.
    pub fn as_opaque_mut(&mut self) -> RefMut<Opaque> {
        self.this.reborrow()
    }

    /// Returns the interface table pointer carried by this handle.
    pub const fn vtable_ptr(&self) -> NonNull<VTable> {
        self.vtable
    }

    /// Returns the interface table carried by this handle.
    pub fn vtable(&self) -> &VTable {
        // SAFETY: All constructors require a non-null vtable pointer that stays
        // valid while methods are called through this handle.
        unsafe { self.vtable.as_ref() }
    }

    /// Erases the vtable type from this bound interface.
    pub fn erase(self) -> ErasedInterfaceRefMut<Opaque> {
        ErasedInterfaceRefMut {
            this: self.this,
            vtable: self.vtable.cast(),
        }
    }
}

/// An ABI-stable exclusive opaque handle bound to an erased runtime-provided interface table.
#[repr(C)]
pub struct ErasedInterfaceRefMut<Opaque: IStable> {
    this: RefMut<Opaque>,
    vtable: NonNull<()>,
}

impl<Opaque: IStable> ErasedInterfaceRefMut<Opaque> {
    /// Creates an erased bound exclusive interface from an opaque handle and erased vtable pointer.
    pub const fn new(this: RefMut<Opaque>, vtable: NonNull<()>) -> Self {
        Self { this, vtable }
    }

    /// Reborrows this erased bound handle as a shared erased bound handle.
    pub fn as_ref(&self) -> ErasedInterfaceRef<Opaque> {
        ErasedInterfaceRef {
            this: self.this.as_ref(),
            vtable: self.vtable,
        }
    }

    /// Reborrows this erased bound handle as an exclusive erased bound handle.
    pub fn reborrow(&mut self) -> ErasedInterfaceRefMut<Opaque> {
        ErasedInterfaceRefMut {
            this: self.this.reborrow(),
            vtable: self.vtable,
        }
    }

    /// Returns the underlying shared opaque handle.
    pub fn as_opaque(&self) -> Ref<Opaque> {
        self.this.as_ref()
    }

    /// Reborrows the underlying exclusive opaque handle.
    pub fn as_opaque_mut(&mut self) -> RefMut<Opaque> {
        self.this.reborrow()
    }

    /// Returns the erased interface table pointer carried by this handle.
    pub const fn vtable_ptr(&self) -> NonNull<()> {
        self.vtable
    }

    /// Rebinds this erased interface to a concrete vtable type.
    ///
    /// # Safety
    /// The vtable pointer must point to a valid `VTable` that remains live for
    /// every method call through the returned handle. Callers should only use
    /// this after the provider has checked `VTable::REPORT` for compatibility.
    pub unsafe fn assume_vtable<VTable: IStable>(self) -> InterfaceRefMut<Opaque, VTable> {
        unsafe { InterfaceRefMut::from_raw(self.this.as_ptr(), self.vtable.cast()) }
    }
}

/// Resolves typed extension interfaces from an opaque runtime capability.
///
/// Implementations should perform report compatibility checks before returning
/// an erased vtable rebound to `VTable`.
pub trait InterfaceResolverMut<Opaque: IStable> {
    /// Resolves a mutable extension interface by its generated vtable type.
    fn resolve_interface<VTable>(&mut self) -> crate::option::Option<InterfaceRefMut<Opaque, VTable>>
    where
        VTable: IStable,
        InterfaceRefMut<Opaque, VTable>: IStable + IDeterminantProvider<()>;
}

const PTR_FIELD: &str = "ptr";
const OPAQUE_FIELD: &str = "opaque";
const THIS_FIELD: &str = "this";
const VTABLE_FIELD: &str = "vtable";

type InterfaceRefLayout<Opaque, VTable> =
    crate::Struct<crate::FieldPair<Ref<Opaque>, NonNull<VTable>>>;
type InterfaceRefMutLayout<Opaque, VTable> =
    crate::Struct<crate::FieldPair<RefMut<Opaque>, NonNull<VTable>>>;
type ErasedInterfaceRefLayout<Opaque> = crate::Struct<crate::FieldPair<Ref<Opaque>, NonNull<()>>>;
type ErasedInterfaceRefMutLayout<Opaque> =
    crate::Struct<crate::FieldPair<RefMut<Opaque>, NonNull<()>>>;

// SAFETY: `Ref` is represented exactly like `NonNull<()>`; the opaque marker is
// included in the report only to distinguish logical opaque APIs.
unsafe impl<Opaque: IStable> IStable for Ref<Opaque> {
    type Size = <NonNull<()> as IStable>::Size;
    type Align = <NonNull<()> as IStable>::Align;
    type ForbiddenValues = <NonNull<()> as IStable>::ForbiddenValues;
    type UnusedBits = <NonNull<()> as IStable>::UnusedBits;
    type HasExactlyOneNiche = B1;
    type ContainsIndirections = B1;
    #[cfg(feature = "experimental-ctypes")]
    type CType = <NonNull<()> as IStable>::CType;
    const REPORT: &'static report::TypeReport = &report::TypeReport {
        name: Str::new("stabby::opaque::Ref"),
        module: Str::new(core::module_path!()),
        fields: StableLike::new(Some(&report::FieldReport {
            name: Str::new(PTR_FIELD),
            ty: <NonNull<()> as IStable>::REPORT,
            next_field: StableLike::new(Some(&report::FieldReport {
                name: Str::new(OPAQUE_FIELD),
                ty: Opaque::REPORT,
                next_field: StableLike::new(None),
            })),
        })),
        version: 0,
        tyty: report::TyTy::Struct,
    };
    const ID: u64 = report::gen_id(Self::REPORT);
}

// SAFETY: `RefMut` is represented exactly like `NonNull<()>`; the opaque marker
// is included in the report only to distinguish logical opaque APIs.
unsafe impl<Opaque: IStable> IStable for RefMut<Opaque> {
    type Size = <NonNull<()> as IStable>::Size;
    type Align = <NonNull<()> as IStable>::Align;
    type ForbiddenValues = <NonNull<()> as IStable>::ForbiddenValues;
    type UnusedBits = <NonNull<()> as IStable>::UnusedBits;
    type HasExactlyOneNiche = B1;
    type ContainsIndirections = B1;
    #[cfg(feature = "experimental-ctypes")]
    type CType = <NonNull<()> as IStable>::CType;
    const REPORT: &'static report::TypeReport = &report::TypeReport {
        name: Str::new("stabby::opaque::RefMut"),
        module: Str::new(core::module_path!()),
        fields: StableLike::new(Some(&report::FieldReport {
            name: Str::new(PTR_FIELD),
            ty: <NonNull<()> as IStable>::REPORT,
            next_field: StableLike::new(Some(&report::FieldReport {
                name: Str::new(OPAQUE_FIELD),
                ty: Opaque::REPORT,
                next_field: StableLike::new(None),
            })),
        })),
        version: 0,
        tyty: report::TyTy::Struct,
    };
    const ID: u64 = report::gen_id(Self::REPORT);
}

// SAFETY: `InterfaceRef` is a `repr(C)` pair of the reported fields.
unsafe impl<Opaque: IStable, VTable: IStable> IStable for InterfaceRef<Opaque, VTable>
where
    InterfaceRefLayout<Opaque, VTable>: IStable,
{
    type Size = <InterfaceRefLayout<Opaque, VTable> as IStable>::Size;
    type Align = <InterfaceRefLayout<Opaque, VTable> as IStable>::Align;
    type ForbiddenValues = <InterfaceRefLayout<Opaque, VTable> as IStable>::ForbiddenValues;
    type UnusedBits = <InterfaceRefLayout<Opaque, VTable> as IStable>::UnusedBits;
    type HasExactlyOneNiche = <InterfaceRefLayout<Opaque, VTable> as IStable>::HasExactlyOneNiche;
    type ContainsIndirections =
        <InterfaceRefLayout<Opaque, VTable> as IStable>::ContainsIndirections;
    #[cfg(feature = "experimental-ctypes")]
    type CType = <InterfaceRefLayout<Opaque, VTable> as IStable>::CType;
    const REPORT: &'static report::TypeReport = &report::TypeReport {
        name: Str::new("stabby::opaque::InterfaceRef"),
        module: Str::new(core::module_path!()),
        fields: StableLike::new(Some(&report::FieldReport {
            name: Str::new(THIS_FIELD),
            ty: <Ref<Opaque> as IStable>::REPORT,
            next_field: StableLike::new(Some(&report::FieldReport {
                name: Str::new(VTABLE_FIELD),
                ty: <NonNull<VTable> as IStable>::REPORT,
                next_field: StableLike::new(None),
            })),
        })),
        version: 0,
        tyty: report::TyTy::Struct,
    };
    const ID: u64 = report::gen_id(Self::REPORT);
}

// SAFETY: `InterfaceRefMut` is a `repr(C)` pair of the reported fields.
unsafe impl<Opaque: IStable, VTable: IStable> IStable for InterfaceRefMut<Opaque, VTable>
where
    InterfaceRefMutLayout<Opaque, VTable>: IStable,
{
    type Size = <InterfaceRefMutLayout<Opaque, VTable> as IStable>::Size;
    type Align = <InterfaceRefMutLayout<Opaque, VTable> as IStable>::Align;
    type ForbiddenValues = <InterfaceRefMutLayout<Opaque, VTable> as IStable>::ForbiddenValues;
    type UnusedBits = <InterfaceRefMutLayout<Opaque, VTable> as IStable>::UnusedBits;
    type HasExactlyOneNiche =
        <InterfaceRefMutLayout<Opaque, VTable> as IStable>::HasExactlyOneNiche;
    type ContainsIndirections =
        <InterfaceRefMutLayout<Opaque, VTable> as IStable>::ContainsIndirections;
    #[cfg(feature = "experimental-ctypes")]
    type CType = <InterfaceRefMutLayout<Opaque, VTable> as IStable>::CType;
    const REPORT: &'static report::TypeReport = &report::TypeReport {
        name: Str::new("stabby::opaque::InterfaceRefMut"),
        module: Str::new(core::module_path!()),
        fields: StableLike::new(Some(&report::FieldReport {
            name: Str::new(THIS_FIELD),
            ty: <RefMut<Opaque> as IStable>::REPORT,
            next_field: StableLike::new(Some(&report::FieldReport {
                name: Str::new(VTABLE_FIELD),
                ty: <NonNull<VTable> as IStable>::REPORT,
                next_field: StableLike::new(None),
            })),
        })),
        version: 0,
        tyty: report::TyTy::Struct,
    };
    const ID: u64 = report::gen_id(Self::REPORT);
}

// SAFETY: `ErasedInterfaceRef` is a `repr(C)` pair of the reported fields.
unsafe impl<Opaque: IStable> IStable for ErasedInterfaceRef<Opaque>
where
    ErasedInterfaceRefLayout<Opaque>: IStable,
{
    type Size = <ErasedInterfaceRefLayout<Opaque> as IStable>::Size;
    type Align = <ErasedInterfaceRefLayout<Opaque> as IStable>::Align;
    type ForbiddenValues = <ErasedInterfaceRefLayout<Opaque> as IStable>::ForbiddenValues;
    type UnusedBits = <ErasedInterfaceRefLayout<Opaque> as IStable>::UnusedBits;
    type HasExactlyOneNiche = <ErasedInterfaceRefLayout<Opaque> as IStable>::HasExactlyOneNiche;
    type ContainsIndirections = <ErasedInterfaceRefLayout<Opaque> as IStable>::ContainsIndirections;
    #[cfg(feature = "experimental-ctypes")]
    type CType = <ErasedInterfaceRefLayout<Opaque> as IStable>::CType;
    const REPORT: &'static report::TypeReport = &report::TypeReport {
        name: Str::new("stabby::opaque::ErasedInterfaceRef"),
        module: Str::new(core::module_path!()),
        fields: StableLike::new(Some(&report::FieldReport {
            name: Str::new(THIS_FIELD),
            ty: <Ref<Opaque> as IStable>::REPORT,
            next_field: StableLike::new(Some(&report::FieldReport {
                name: Str::new(VTABLE_FIELD),
                ty: <NonNull<()> as IStable>::REPORT,
                next_field: StableLike::new(None),
            })),
        })),
        version: 0,
        tyty: report::TyTy::Struct,
    };
    const ID: u64 = report::gen_id(Self::REPORT);
}

// SAFETY: `ErasedInterfaceRefMut` is a `repr(C)` pair of the reported fields.
unsafe impl<Opaque: IStable> IStable for ErasedInterfaceRefMut<Opaque>
where
    ErasedInterfaceRefMutLayout<Opaque>: IStable,
{
    type Size = <ErasedInterfaceRefMutLayout<Opaque> as IStable>::Size;
    type Align = <ErasedInterfaceRefMutLayout<Opaque> as IStable>::Align;
    type ForbiddenValues = <ErasedInterfaceRefMutLayout<Opaque> as IStable>::ForbiddenValues;
    type UnusedBits = <ErasedInterfaceRefMutLayout<Opaque> as IStable>::UnusedBits;
    type HasExactlyOneNiche = <ErasedInterfaceRefMutLayout<Opaque> as IStable>::HasExactlyOneNiche;
    type ContainsIndirections =
        <ErasedInterfaceRefMutLayout<Opaque> as IStable>::ContainsIndirections;
    #[cfg(feature = "experimental-ctypes")]
    type CType = <ErasedInterfaceRefMutLayout<Opaque> as IStable>::CType;
    const REPORT: &'static report::TypeReport = &report::TypeReport {
        name: Str::new("stabby::opaque::ErasedInterfaceRefMut"),
        module: Str::new(core::module_path!()),
        fields: StableLike::new(Some(&report::FieldReport {
            name: Str::new(THIS_FIELD),
            ty: <RefMut<Opaque> as IStable>::REPORT,
            next_field: StableLike::new(Some(&report::FieldReport {
                name: Str::new(VTABLE_FIELD),
                ty: <NonNull<()> as IStable>::REPORT,
                next_field: StableLike::new(None),
            })),
        })),
        version: 0,
        tyty: report::TyTy::Struct,
    };
    const ID: u64 = report::gen_id(Self::REPORT);
}
