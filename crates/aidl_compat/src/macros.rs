//! Macro definitions conforming to official AOSP `aidl --lang=rust` code generation.

/// Declares an AIDL interface, including its descriptor, proxy (`Bp*`), and stub generator (`Bn*`).
#[macro_export]
macro_rules! declare_binder_interface {
    // Pattern 1: native with on_transact fn, proxy with struct fields
    {
        $interface:ident [$descriptor:expr] {
            native: $native:ident($on_transact:path),
            proxy: $proxy:ident $( { $($proxy_fields:tt)* } )?,
            $(async: $async_interface:ident,)?
        }
    } => {
        $crate::_declare_binder_interface_internal! {
            $interface, $descriptor, $native, $on_transact, $proxy $(, $($proxy_fields)* )?
        }
    };

    // Pattern 2: native with on_transact fn, proxy without trailing comma/braces
    {
        $interface:ident [$descriptor:expr] {
            native: $native:ident($on_transact:path),
            proxy: $proxy:ident
        }
    } => {
        $crate::_declare_binder_interface_internal! {
            $interface, $descriptor, $native, $on_transact, $proxy
        }
    };

    // Pattern 3: native without on_transact fn (defaults to on_transact)
    {
        $interface:ident [$descriptor:expr] {
            native: $native:ident,
            proxy: $proxy:ident $( { $($proxy_fields:tt)* } )?,
            $(async: $async_interface:ident,)?
        }
    } => {
        $crate::_declare_binder_interface_internal! {
            $interface, $descriptor, $native, on_transact, $proxy $(, $($proxy_fields)* )?
        }
    };

    // Pattern 4: native without on_transact, simple proxy
    {
        $interface:ident [$descriptor:expr] {
            native: $native:ident,
            proxy: $proxy:ident
        }
    } => {
        $crate::_declare_binder_interface_internal! {
            $interface, $descriptor, $native, on_transact, $proxy
        }
    };
}

/// Internal macro expansion helper for `declare_binder_interface!`.
#[macro_export]
#[doc(hidden)]
macro_rules! _declare_binder_interface_internal {
    (
        $interface:ident,
        $descriptor:expr,
        $native:ident,
        $on_transact:path,
        $proxy:ident
        $(, $($proxy_fields:tt)* )?
    ) => {
        /// Client-side AIDL proxy wrapper.
        pub struct $proxy {
            pub(crate) binder: $crate::SpIBinder,
            $( $($proxy_fields)* )?
        }

        impl $proxy {
            /// Construct a new proxy wrapping an `SpIBinder`.
            pub fn new(binder: $crate::SpIBinder) -> Self {
                Self {
                    binder,
                    $( $($proxy_fields)* )?
                }
            }
        }

        impl $crate::Interface for $proxy {
            fn as_binder(&self) -> $crate::SpIBinder {
                self.binder.clone()
            }
        }

        impl $crate::Proxy for $proxy {
            fn as_binder(&self) -> &$crate::SpIBinder {
                &self.binder
            }
        }

        impl $crate::FromIBinder for dyn $interface {
            fn try_from(binder: $crate::SpIBinder) -> $crate::Result<$crate::Strong<dyn $interface>> {
                Ok($crate::Strong::new(std::sync::Arc::new($proxy::new(binder))))
            }
        }

        impl $crate::FromIBinder for $proxy {
            fn try_from(binder: $crate::SpIBinder) -> $crate::Result<$crate::Strong<$proxy>> {
                Ok($crate::Strong::new(std::sync::Arc::new($proxy::new(binder))))
            }
        }

        /// Server-side AIDL stub helper for creating Binder endpoints.
        pub struct $native<T = ()> {
            inner: std::sync::Arc<T>,
        }

        impl<T: $interface + 'static> $crate::Remotable for $native<T> {
            fn get_class_descriptor() -> &'static str {
                $descriptor
            }

            fn on_transact(
                &self,
                code: $crate::TransactionCode,
                data: &$crate::Parcel,
                reply: &mut $crate::Parcel,
            ) -> $crate::Result<()> {
                $on_transact(&*self.inner, code, data, reply)
            }
        }

        impl $native {
            /// Wrap an implementation of `$interface` into an `SpIBinder` with default features.
            pub fn new_binder<T: $interface + 'static>(
                inner: T,
                _features: $crate::BinderFeatures,
            ) -> $crate::SpIBinder {
                $crate::Binder::new($native {
                    inner: std::sync::Arc::new(inner),
                })
            }

            /// Wrap an `Arc` implementation of `$interface` into an `SpIBinder` with default features.
            pub fn new_binder_arc<T: $interface + 'static>(
                inner: std::sync::Arc<T>,
                _features: $crate::BinderFeatures,
            ) -> $crate::SpIBinder {
                $crate::Binder::new($native {
                    inner,
                })
            }
        }
    };
}

/// Declares an AIDL enum with backing integer type and Parcelable implementation.
#[macro_export]
macro_rules! declare_binder_enum {
    {
        $( #[$meta:meta] )*
        $name:ident : [$backing:ident; $count:expr] {
            $( $variant:ident = $val:expr, )*
        }
    } => {
        $( #[$meta] )*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr($backing)]
        pub enum $name {
            $( $variant = $val, )*
        }

        impl $name {
            /// Write enum to parcel.
            pub fn write_to_parcel(&self, parcel: &mut $crate::Parcel) -> $crate::Result<()> {
                <$name as $crate::Parcelable>::write_to_parcel(self, parcel)
            }

            /// Read enum from parcel.
            pub fn read_from_parcel(&mut self, parcel: &$crate::Parcel) -> $crate::Result<()> {
                <$name as $crate::Parcelable>::read_from_parcel(self, parcel)
            }

            /// Read enum from parcel at specified offset.
            pub fn read_from_parcel_at(&mut self, parcel: &$crate::Parcel, offset: &mut usize) -> $crate::Result<()> {
                <$name as $crate::Parcelable>::read_from_parcel_at(self, parcel, offset)
            }
        }

        impl $crate::Parcelable for $name {
            fn write_to_parcel(&self, parcel: &mut $crate::Parcel) -> $crate::Result<()> {
                parcel.write_i32(*self as i32)
                    .map_err(|_| $crate::Status::from_status($crate::STATUS_BAD_VALUE))
            }

            fn read_from_parcel(&mut self, parcel: &$crate::Parcel) -> $crate::Result<()> {
                let mut offset = 0;
                self.read_from_parcel_at(parcel, &mut offset)
            }

            fn read_from_parcel_at(&mut self, parcel: &$crate::Parcel, offset: &mut usize) -> $crate::Result<()> {
                let val = parcel.read_i32(offset)
                    .map_err(|_| $crate::Status::from_status($crate::STATUS_BAD_VALUE))?;
                $(
                    if val == ($val as i32) {
                        *self = $name::$variant;
                        return Ok(());
                    }
                )*
                Err($crate::Status::from_status($crate::STATUS_BAD_VALUE))
            }
        }
    };

    {
        $( #[$meta:meta] )*
        $name:ident : $backing:ident {
            $( $variant:ident = $val:expr, )*
        }
    } => {
        $crate::declare_binder_enum! {
            $( #[$meta] )*
            $name : [$backing; 0] {
                $( $variant = $val, )*
            }
        }
    };
}

/// Implements `Parcelable` for custom struct types.
#[macro_export]
macro_rules! impl_binder_interface {
    ($type:ty, $descriptor:expr) => {
        impl $crate::Interface for $type {
            fn as_binder(&self) -> $crate::SpIBinder {
                $crate::SpIBinder::new(self.clone())
            }
        }
    };
}
