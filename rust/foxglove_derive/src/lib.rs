extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use std::collections::HashMap;
use syn::{
    Data, DataEnum, DataStruct, DeriveInput, Fields, GenericArgument, GenericParam, Generics,
    PathArguments, Type, parse_macro_input, parse_quote,
};

/// Extract the inner type from a wrapper type like `Vec<T>` or `Option<T>`.
/// Returns the wrapper name and inner type if it matches the pattern.
fn unwrap_generic_type<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner_ty) => Some(inner_ty),
        _ => None,
    }
}

/// Check if a type is `Vec<T>` for some T.
fn is_vec(ty: &Type) -> bool {
    unwrap_generic_type(ty, "Vec").is_some()
}

/// Check if a type is `Option<T>` for some T.
fn is_option(ty: &Type) -> bool {
    unwrap_generic_type(ty, "Option").is_some()
}

/// Check if a type is `[T; N]` for some T and N.
fn is_array(ty: &Type) -> bool {
    matches!(ty, Type::Array(_))
}

/// Extract the element type from an array type `[T; N]`.
fn unwrap_array_type(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Array(arr) => Some(&arr.elem),
        _ => None,
    }
}

/// Check if a type is `Vec<Option<T>>`, which is not supported because protobuf
/// repeated fields cannot represent null/missing elements.
fn is_vec_of_option(ty: &Type) -> bool {
    unwrap_generic_type(ty, "Vec").is_some_and(is_option)
}

/// Check if a type is `[Option<T>; N]`, which is not supported because protobuf
/// repeated fields cannot represent null/missing elements.
fn is_array_of_option(ty: &Type) -> bool {
    unwrap_array_type(ty).is_some_and(is_option)
}

/// Check if a type is `Option<Vec<T>>`, which is not supported because protobuf
/// cannot distinguish between "not present" and "empty list".
fn is_option_of_vec(ty: &Type) -> bool {
    unwrap_generic_type(ty, "Option").is_some_and(is_vec)
}

/// Check if a type is `Option<[T; N]>`, which is not supported because protobuf
/// cannot distinguish between "not present" and "empty array".
fn is_option_of_array(ty: &Type) -> bool {
    unwrap_generic_type(ty, "Option").is_some_and(is_array)
}

/// Check if a type is `Vec<Vec<T>>`, which is not supported because protobuf
/// does not support nested repeated fields.
fn is_vec_of_vec(ty: &Type) -> bool {
    unwrap_generic_type(ty, "Vec").is_some_and(is_vec)
}

/// Check if a type is `[Vec<T>; N]`, which is not supported because protobuf
/// does not support nested repeated fields.
fn is_array_of_vec(ty: &Type) -> bool {
    unwrap_array_type(ty).is_some_and(is_vec)
}

/// Check if a type is `Vec<[T; N]>`, which is not supported because protobuf
/// does not support nested repeated fields.
fn is_vec_of_array(ty: &Type) -> bool {
    unwrap_generic_type(ty, "Vec").is_some_and(is_array)
}

/// Check if a type is `[[T; M]; N]`, which is not supported because protobuf
/// does not support nested repeated fields.
fn is_array_of_array(ty: &Type) -> bool {
    unwrap_array_type(ty).is_some_and(is_array)
}

type TypeCheck = (fn(&Type) -> bool, &'static str);

/// Validate that a type does not use unsupported nesting patterns.
/// Returns `Some(compile_error!(...))` if the type is invalid, `None` if valid.
fn validate_field_type(ty: &Type) -> Option<TokenStream> {
    let checks: &[TypeCheck] = &[
        (
            is_vec_of_option,
            "Vec<Option<T>> is not supported. Protobuf repeated fields cannot represent null/missing elements.",
        ),
        (
            is_option_of_vec,
            "Option<Vec<T>> is not supported. Protobuf cannot distinguish between absent and empty repeated fields.",
        ),
        (
            is_vec_of_vec,
            "Vec<Vec<T>> is not supported. Protobuf does not support nested repeated fields.",
        ),
        (
            is_array_of_option,
            "[Option<T>; N] is not supported. Protobuf repeated fields cannot represent null/missing elements.",
        ),
        (
            is_option_of_array,
            "Option<[T; N]> is not supported. Protobuf cannot distinguish between absent and empty repeated fields.",
        ),
        (
            is_array_of_array,
            "[[T; M]; N] is not supported. Protobuf does not support nested repeated fields.",
        ),
        (
            is_array_of_vec,
            "[Vec<T>; N] is not supported. Protobuf does not support nested repeated fields.",
        ),
        (
            is_vec_of_array,
            "Vec<[T; N]> is not supported. Protobuf does not support nested repeated fields.",
        ),
    ];
    for &(check, msg) in checks {
        if check(ty) {
            return Some(TokenStream::from(quote! { compile_error!(#msg); }));
        }
    }
    None
}

/// Derive macro for enums and structs allowing them to be logged to a Foxglove channel.
///
/// This is a convenience for getting data into Foxglove with minimal friction. It generates a
/// schema and serialization code automatically based on your type's fields. The underlying
/// serialization format is an implementation detail and may change across SDK versions.
///
/// **Important:** The derived schema is not designed for schema evolution. Reordering, inserting,
/// or removing fields will silently break compatibility with previously recorded data.
///
/// If you need backwards-compatible schemas, maintain an explicit `.proto` file and use a library
/// like [prost](https://docs.rs/prost) to generate your types. You can then implement
/// `Encode` manually for those types.
#[proc_macro_derive(Encode)]
pub fn derive_loggable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match &input.data {
        Data::Enum(data) => derive_enum_impl(&input, data),
        Data::Struct(data) => derive_struct_impl(&input, data),
        _ => TokenStream::from(quote! {
            compile_error!("Encode can only be used with enums or structs");
        }),
    }
}

fn derive_enum_impl(input: &DeriveInput, data: &DataEnum) -> TokenStream {
    let name = &input.ident;
    let variants = &data.variants;

    for variant in variants {
        if !variant.fields.is_empty() {
            return TokenStream::from(quote! {
                compile_error!("Enums with associated data are not supported.");
            });
        }
    }

    // Generate variant name and number pairs for enum descriptor
    let variant_descriptors = variants.iter().enumerate().map(|(i, v)| {
        let variant_ident = &v.ident;
        let variant_name = variant_ident.to_string();

        let variant_value = i as i32;

        quote! {
            let mut value = ::foxglove::prost_types::EnumValueDescriptorProto::default();
            value.name = Some(String::from(#variant_name));
            value.number = Some(#variant_value as i32);
            enum_desc.value.push(value);
        }
    });

    // Generate implementation
    let expanded = quote! {
        #[automatically_derived]
        impl ::foxglove::protobuf::ProtobufField for #name {
            fn field_type() -> ::foxglove::prost_types::field_descriptor_proto::Type {
                ::foxglove::prost_types::field_descriptor_proto::Type::Enum
            }

            fn wire_type() -> u32 {
                0 // Varint, same as integers
            }

            fn write(&self, buf: &mut impl ::foxglove::bytes::BufMut) {
                ::foxglove::protobuf::encode_varint(*self as u64, buf);
            }

            fn enum_descriptor() -> Option<::foxglove::prost_types::EnumDescriptorProto> {
                let mut enum_desc = ::foxglove::prost_types::EnumDescriptorProto::default();
                enum_desc.name = Some(stringify!(#name).to_string());

                #(#variant_descriptors)*

                Some(enum_desc)
            }

            fn type_name() -> Option<String> {
                Some(stringify!(#name).to_string())
            }

            fn encoded_len(&self) -> usize {
                ::foxglove::protobuf::encoded_len_varint(*self as u64)
            }
        }
    };

    TokenStream::from(expanded)
}

// Add a bound `T: ProtobufField` to every type parameter T.
fn add_protobuf_bound(mut generics: Generics) -> Generics {
    for param in &mut generics.params {
        if let GenericParam::Type(ref mut type_param) = *param {
            type_param
                .bounds
                .push(parse_quote!(::foxglove::protobuf::ProtobufField));
        }
    }
    generics
}

fn derive_struct_impl(input: &DeriveInput, data: &DataStruct) -> TokenStream {
    match &data.fields {
        Fields::Named(fields) => derive_named_struct_impl(input, &fields.named),
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
            derive_newtype_impl(input, &fields.unnamed[0])
        }
        _ => TokenStream::from(quote! {
            compile_error!("Only named struct fields and single-element tuple structs are supported");
        }),
    }
}

/// Generate a transparent `ProtobufField` implementation and a standalone `Encode`
/// implementation for a newtype wrapper.
///
/// For `struct Foo(T)` where `T: ProtobufField`, the `ProtobufField` impl delegates
/// all trait methods to `T`, making the newtype transparent when used as a field in
/// other structs. The `Encode` impl treats `Foo` as a single-field protobuf message
/// with the field named `"value"` at field number 1, allowing it to be used as a
/// standalone top-level message.
fn derive_newtype_impl(input: &DeriveInput, field: &syn::Field) -> TokenStream {
    let name = &input.ident;
    let inner_type = &field.ty;

    let name_str = name.to_string();
    let package_name = name_str.to_lowercase();
    let full_name = format!("{package_name}.{name_str}");

    if let Some(err) = validate_field_type(inner_type) {
        return err;
    }

    let generics = add_protobuf_bound(input.generics.clone());
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let has_generics = !input.generics.params.is_empty();

    // Extract the schema computation body so we can conditionally wrap it
    // with OnceLock caching. For generic types, static items inside generic
    // functions are shared across all monomorphizations, so we must not cache.
    let newtype_schema_body = quote! {
        let mut file_descriptor_set = ::foxglove::prost_types::FileDescriptorSet::default();

        // Add file descriptors for well-known types and nested message dependencies.
        // Deduplicate by name since multiple fields may reference the same WKT.
        let dependency_fds = <#inner_type as ::foxglove::protobuf::ProtobufField>::file_descriptors();
        let mut seen = ::std::collections::HashSet::new();
        let mut dependencies = Vec::new();
        for fd in dependency_fds {
            if let Some(fd_name) = &fd.name {
                if seen.insert(fd_name.clone()) {
                    dependencies.push(fd_name.clone());
                    file_descriptor_set.file.push(fd);
                }
            }
        }

        let mut file = ::foxglove::prost_types::FileDescriptorProto {
            name: Some(String::from(concat!(stringify!(#name), ".proto"))),
            package: Some(String::from(#package_name)),
            syntax: Some(String::from("proto3")),
            dependency: dependencies,
            ..Default::default()
        };

        // Build the message descriptor inline rather than delegating to
        // Self::message_descriptor(), which is transparent and returns the
        // inner type's descriptor.
        let mut message = ::foxglove::prost_types::DescriptorProto::default();
        message.name = Some(String::from(stringify!(#name)));

        // Single field: "value" at field number 1
        let mut field_desc = ::foxglove::prost_types::FieldDescriptorProto::default();
        field_desc.name = Some(String::from("value"));
        field_desc.number = Some(1);

        if <#inner_type as ::foxglove::protobuf::ProtobufField>::repeating() {
            field_desc.label = Some(::foxglove::prost_types::field_descriptor_proto::Label::Repeated as i32);
        } else if <#inner_type as ::foxglove::protobuf::ProtobufField>::optional() {
            field_desc.label = Some(::foxglove::prost_types::field_descriptor_proto::Label::Optional as i32);
        } else {
            field_desc.label = Some(::foxglove::prost_types::field_descriptor_proto::Label::Required as i32);
        }
        field_desc.r#type = Some(<#inner_type as ::foxglove::protobuf::ProtobufField>::field_type() as i32);
        field_desc.type_name = <#inner_type as ::foxglove::protobuf::ProtobufField>::type_name();

        message.field.push(field_desc);

        if let Some(enum_desc) = <#inner_type as ::foxglove::protobuf::ProtobufField>::enum_descriptor() {
            message.enum_type.push(enum_desc);
        }

        if let Some(message_desc) = <#inner_type as ::foxglove::protobuf::ProtobufField>::message_descriptor() {
            message.nested_type.push(message_desc);
        }

        file.message_type.push(message);
        file_descriptor_set.file.push(file);

        let bytes = ::foxglove::protobuf::prost_file_descriptor_set_to_vec(&file_descriptor_set);

        Some(::foxglove::Schema {
            name: String::from(#full_name),
            encoding: String::from("protobuf"),
            data: std::borrow::Cow::Owned(bytes),
        })
    };

    let newtype_get_schema = if has_generics {
        quote! {
            fn get_schema() -> Option<::foxglove::Schema> {
                #newtype_schema_body
            }
        }
    } else {
        quote! {
            fn get_schema() -> Option<::foxglove::Schema> {
                static SCHEMA: ::std::sync::OnceLock<Option<::foxglove::Schema>> = ::std::sync::OnceLock::new();
                SCHEMA.get_or_init(|| {
                    #newtype_schema_body
                }).clone()
            }
        }
    };

    let expanded = quote! {
        #[automatically_derived]
        impl #impl_generics ::foxglove::protobuf::ProtobufField for #name #ty_generics #where_clause {
            fn field_type() -> ::foxglove::prost_types::field_descriptor_proto::Type {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::field_type()
            }

            fn wire_type() -> u32 {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::wire_type()
            }

            fn write_tagged(&self, field_number: u32, buf: &mut impl ::foxglove::bytes::BufMut) {
                ::foxglove::protobuf::ProtobufField::write_tagged(&self.0, field_number, buf)
            }

            fn write(&self, buf: &mut impl ::foxglove::bytes::BufMut) {
                ::foxglove::protobuf::ProtobufField::write(&self.0, buf)
            }

            fn type_name() -> Option<String> {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::type_name()
            }

            fn enum_descriptor() -> Option<::foxglove::prost_types::EnumDescriptorProto> {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::enum_descriptor()
            }

            fn message_descriptor() -> Option<::foxglove::prost_types::DescriptorProto> {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::message_descriptor()
            }

            fn file_descriptor() -> Option<::foxglove::prost_types::FileDescriptorProto> {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::file_descriptor()
            }

            fn file_descriptors() -> Vec<::foxglove::prost_types::FileDescriptorProto> {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::file_descriptors()
            }

            fn repeating() -> bool {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::repeating()
            }

            fn optional() -> bool {
                <#inner_type as ::foxglove::protobuf::ProtobufField>::optional()
            }

            fn encoded_len(&self) -> usize {
                ::foxglove::protobuf::ProtobufField::encoded_len(&self.0)
            }

            fn encoded_len_tagged(&self, field_number: u32) -> usize {
                ::foxglove::protobuf::ProtobufField::encoded_len_tagged(&self.0, field_number)
            }
        }

        #[automatically_derived]
        impl #impl_generics ::foxglove::Encode for #name #ty_generics #where_clause {
            type Error = ::foxglove::FoxgloveError;

            #newtype_get_schema

            fn get_message_encoding() -> String {
                String::from("protobuf")
            }

            fn encode(&self, buf: &mut impl ::foxglove::bytes::BufMut) -> Result<(), Self::Error> {
                if self.encoded_len().is_some_and(|len| len > buf.remaining_mut()) {
                    return Err(::foxglove::FoxgloveError::EncodeError(
                        "insufficient buffer".to_string(),
                    ));
                }

                // The top level message is encoded without a length prefix
                ::foxglove::protobuf::ProtobufField::write_tagged(&self.0, 1u32, buf);
                Ok(())
            }

            fn encoded_len(&self) -> Option<usize> {
                Some(::foxglove::protobuf::ProtobufField::encoded_len_tagged(&self.0, 1u32))
            }
        }
    };

    TokenStream::from(expanded)
}

fn derive_named_struct_impl(
    input: &DeriveInput,
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
) -> TokenStream {
    let name = &input.ident;
    let name_str = name.to_string();
    let package_name = name_str.to_lowercase();
    let full_name = format!("{package_name}.{name_str}");

    let generics = add_protobuf_bound(input.generics.clone());
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let mut field_defs = Vec::new();
    let mut field_encoders = Vec::new();
    let mut field_tagged_lengths = Vec::new();

    // Field number + wire type must fit into a u32, but there is also a much lower reserved
    // range starting at 19,000. We should need to encode a much smaller space in practice; if
    // we limit to 2047, then each encoded tag will take at most two bytes.
    // https://protobuf.dev/programming-guides/proto3/#assigning
    let max_field_number = 2047;

    // If a struct nests multiple values of the same enum or message type, we
    // only define them once, based on name.
    let mut enum_defs: HashMap<&syn::Type, proc_macro2::TokenStream> = HashMap::new();
    let mut message_defs: HashMap<&syn::Type, proc_macro2::TokenStream> = HashMap::new();
    let mut file_defs: HashMap<&syn::Type, proc_macro2::TokenStream> = HashMap::new();

    for (i, field) in fields.iter().enumerate() {
        let field_name = &field.ident.as_ref().unwrap();
        let field_type = &field.ty;
        let field_number = i as u32 + 1;

        if field_number > max_field_number {
            return TokenStream::from(quote! {
                compile_error!("Too many fields to encode");
            });
        }

        if let Some(err) = validate_field_type(field_type) {
            return err;
        }

        field_tagged_lengths.push(quote! {
            ::foxglove::protobuf::ProtobufField::encoded_len_tagged(&self.#field_name, #field_number)
        });

        enum_defs.entry(field_type).or_insert_with(|| quote! {
            if let Some(enum_desc) = <#field_type as ::foxglove::protobuf::ProtobufField>::enum_descriptor() {
                enum_type.push(enum_desc);
            }
        });

        message_defs.entry(field_type).or_insert_with(|| quote! {
            if let Some(message_descriptor) = <#field_type as ::foxglove::protobuf::ProtobufField>::message_descriptor() {
                nested_type.push(message_descriptor);
            }
        });

        file_defs.entry(field_type).or_insert_with(|| {
            quote! {
                for fd in <#field_type as ::foxglove::protobuf::ProtobufField>::file_descriptors() {
                    result.push(fd);
                }
            }
        });

        field_defs.push(quote! {
            let mut field = ::foxglove::prost_types::FieldDescriptorProto::default();
            field.name = Some(String::from(stringify!(#field_name)));
            field.number = Some(#field_number as i32);

            if <#field_type as ::foxglove::protobuf::ProtobufField>::repeating() {
                field.label = Some(::foxglove::prost_types::field_descriptor_proto::Label::Repeated as i32);
            } else if <#field_type as ::foxglove::protobuf::ProtobufField>::optional() {
                field.label = Some(::foxglove::prost_types::field_descriptor_proto::Label::Optional as i32);
            } else {
                field.label = Some(::foxglove::prost_types::field_descriptor_proto::Label::Required as i32);
            }
            field.r#type = Some(<#field_type as ::foxglove::protobuf::ProtobufField>::field_type() as i32);

            field.type_name = <#field_type as ::foxglove::protobuf::ProtobufField>::type_name();

            message.field.push(field);
        });

        field_encoders.push(quote! {
            ::foxglove::protobuf::ProtobufField::write_tagged(&self.#field_name, #field_number, buf);
        });
    }

    let enum_defs = enum_defs.into_values().collect::<Vec<_>>();
    let message_defs = message_defs.into_values().collect::<Vec<_>>();
    let file_defs = file_defs.into_values().collect::<Vec<_>>();
    let has_generics = !input.generics.params.is_empty();

    // Extract computation bodies so we can conditionally wrap with OnceLock
    // caching. For generic types, static items inside generic functions are
    // shared across all monomorphizations, so we must not cache.
    let message_descriptor_body = quote! {
        let mut message = ::foxglove::prost_types::DescriptorProto::default();
        message.name = Some(String::from(stringify!(#name)));

        #(#field_defs)*

        {
            let mut enum_type = &mut message.enum_type;
            #(#enum_defs)*
        }

        {
            let mut nested_type = &mut message.nested_type;
            #(#message_defs)*
        }

        Some(message)
    };

    let message_descriptor_method = if has_generics {
        quote! {
            fn message_descriptor() -> Option<::foxglove::prost_types::DescriptorProto> {
                #message_descriptor_body
            }
        }
    } else {
        quote! {
            fn message_descriptor() -> Option<::foxglove::prost_types::DescriptorProto> {
                static DESCRIPTOR: ::std::sync::OnceLock<Option<::foxglove::prost_types::DescriptorProto>> = ::std::sync::OnceLock::new();
                DESCRIPTOR.get_or_init(|| {
                    #message_descriptor_body
                }).clone()
            }
        }
    };

    let named_schema_body = quote! {
        let mut file_descriptor_set = ::foxglove::prost_types::FileDescriptorSet::default();

        // Add file descriptors for well-known types and nested message dependencies.
        // Deduplicate by name since multiple fields may reference the same WKT.
        let dependency_fds = <#name #ty_generics as ::foxglove::protobuf::ProtobufField>::file_descriptors();
        let mut seen = ::std::collections::HashSet::new();
        let mut dependencies = Vec::new();
        for fd in dependency_fds {
            if let Some(fd_name) = &fd.name {
                if seen.insert(fd_name.clone()) {
                    dependencies.push(fd_name.clone());
                    file_descriptor_set.file.push(fd);
                }
            }
        }

        let mut file = ::foxglove::prost_types::FileDescriptorProto {
            name: Some(String::from(concat!(stringify!(#name), ".proto"))),
            package: Some(String::from(#package_name)),
            syntax: Some(String::from("proto3")),
            dependency: dependencies,
            ..Default::default()
        };

        if let Some(message_descriptor) = <#name #ty_generics as ::foxglove::protobuf::ProtobufField>::message_descriptor() {
            file.message_type.push(message_descriptor);
        }

        file_descriptor_set.file.push(file);

        let bytes = ::foxglove::protobuf::prost_file_descriptor_set_to_vec(&file_descriptor_set);

        Some(::foxglove::Schema {
            name: String::from(#full_name),
            encoding: String::from("protobuf"),
            data: std::borrow::Cow::Owned(bytes),
        })
    };

    let named_get_schema = if has_generics {
        quote! {
            fn get_schema() -> Option<::foxglove::Schema> {
                #named_schema_body
            }
        }
    } else {
        quote! {
            fn get_schema() -> Option<::foxglove::Schema> {
                static SCHEMA: ::std::sync::OnceLock<Option<::foxglove::Schema>> = ::std::sync::OnceLock::new();
                SCHEMA.get_or_init(|| {
                    #named_schema_body
                }).clone()
            }
        }
    };

    // Generate the output tokens
    let expanded = quote! {
        #[automatically_derived]
        impl #impl_generics ::foxglove::protobuf::ProtobufField for #name #ty_generics #where_clause {
            fn field_type() -> ::foxglove::prost_types::field_descriptor_proto::Type {
                ::foxglove::prost_types::field_descriptor_proto::Type::Message
            }

            fn wire_type() -> u32 {
                2 // Length-delimited, same as strings and bytes
            }

            fn write(&self, out: &mut impl ::foxglove::bytes::BufMut) {
                use ::foxglove::bytes::BufMut;

                let mut local_buf = vec![];

                // make a mutable reference to buf because field_encoders needs a mutable reference
                // for the generated code
                let mut buf = &mut local_buf;

                // Encode each field using proper protobuf encoding
                #(#field_encoders)*

                // Write the length as a varint
                let len = buf.len();
                ::foxglove::protobuf::encode_varint(len as u64, out);

                out.put_slice(&buf);
            }

            #message_descriptor_method

            fn type_name() -> Option<String> {
                Some(stringify!(#name).to_string())
            }

            fn file_descriptors() -> Vec<::foxglove::prost_types::FileDescriptorProto> {
                let mut result = Vec::new();
                #(#file_defs)*
                result
            }

            fn encoded_len(&self) -> usize {
                0 #(+ #field_tagged_lengths)*
            }
        }

        #[automatically_derived]
        impl #impl_generics ::foxglove::Encode for #name #ty_generics #where_clause {
            type Error = ::foxglove::FoxgloveError;

            #named_get_schema

            fn get_message_encoding() -> String {
                String::from("protobuf")
            }

            fn encode(&self, buf: &mut impl ::foxglove::bytes::BufMut) -> Result<(), Self::Error> {
                if self.encoded_len().is_some_and(|len| len > buf.remaining_mut()) {
                    return Err(::foxglove::FoxgloveError::EncodeError(
                        "insufficient buffer".to_string(),
                    ));
                }

                // The top level message is encoded without a length prefix
                #(#field_encoders)*
                Ok(())
            }

            fn encoded_len(&self) -> Option<usize> {
                Some(0 #(+ #field_tagged_lengths)*)
            }
        }
    };

    TokenStream::from(expanded)
}
