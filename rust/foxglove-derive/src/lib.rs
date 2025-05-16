extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use std::collections::HashMap;
use syn::{
    parse_macro_input, parse_quote, Data, DataEnum, DataStruct, DeriveInput, Fields, GenericParam,
    Generics,
};

/// Derive macro for enums and structs allowing them to be logged to a Foxglove channel.
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
    let name = &input.ident;
    let name_str = name.to_string();
    let package_name = name_str.to_lowercase();
    let full_name = format!("{package_name}.{name_str}");

    let generics = add_protobuf_bound(input.generics.clone());
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Extract fields from the struct
    let fields = match &data.fields {
        Fields::Named(fields) => &fields.named,
        _ => {
            return TokenStream::from(quote! {
                compile_error!("Only named struct fields are supported");
            })
        }
    };

    let mut field_defs = Vec::new();
    let mut field_encoders = Vec::new();
    let mut field_message_lengths = Vec::new();

    // Field number + wire type must fit into a u32, but there is also a much lower reserved
    // range starting at 19,000. We should need to encode a much smaller space in practice; if
    // we limit to 2047, then each encoded tag will take at most two bytes.
    // Fields 1-15 use a single byte for the tag; fields 16-2047 use two bytes.
    // https://protobuf.dev/programming-guides/proto3/#assigning
    let max_one_byte_field_number = 15;
    let max_field_number = 2047;
    let mut tags_encoded_len: usize = 0;

    // If a struct nests multiple values of the same enum or message type, we
    // only define them once, based on name.
    let mut enum_defs: HashMap<&syn::Type, proc_macro2::TokenStream> = HashMap::new();
    let mut message_defs: HashMap<&syn::Type, proc_macro2::TokenStream> = HashMap::new();

    for (i, field) in fields.iter().enumerate() {
        let field_name = &field.ident.as_ref().unwrap();
        let field_type = &field.ty;
        let field_number = i as u32 + 1;

        if field_number > max_field_number {
            return TokenStream::from(quote! {
                compile_error!("Too many fields to encode");
            });
        }

        field_message_lengths.push(quote! {
            ::foxglove::protobuf::ProtobufField::encoded_len(&self.#field_name)
        });

        tags_encoded_len += if field_number <= max_one_byte_field_number {
            1
        } else {
            2
        };

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

        field_defs.push(quote! {
            let mut field = ::foxglove::prost_types::FieldDescriptorProto::default();
            field.name = Some(String::from(stringify!(#field_name)));
            field.number = Some(#field_number as i32);

            if <#field_type as ::foxglove::protobuf::ProtobufField>::repeating() {
                field.label = Some(::foxglove::prost_types::field_descriptor_proto::Label::Repeated as i32);
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

            fn message_descriptor() -> Option<::foxglove::prost_types::DescriptorProto> {
                static DESCRIPTOR: ::std::sync::OnceLock<Option<::foxglove::prost_types::DescriptorProto>> = ::std::sync::OnceLock::new();
                DESCRIPTOR.get_or_init(|| {
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
                }).clone()
            }

            fn type_name() -> Option<String> {
                Some(stringify!(#name).to_string())
            }

            fn encoded_len(&self) -> usize {
                #tags_encoded_len #(+ #field_message_lengths)*
            }
        }

        #[automatically_derived]
        impl #impl_generics ::foxglove::Encode for #name #ty_generics #where_clause {
            type Error = ::foxglove::FoxgloveError;

            fn get_schema() -> Option<::foxglove::Schema> {
                static SCHEMA: ::std::sync::OnceLock<Option<::foxglove::Schema>> = ::std::sync::OnceLock::new();
                SCHEMA.get_or_init(|| {
                    let mut file_descriptor_set = ::foxglove::prost_types::FileDescriptorSet::default();
                    let mut file = ::foxglove::prost_types::FileDescriptorProto {
                        name: Some(String::from(concat!(stringify!(#name), ".proto"))),
                        package: Some(String::from(stringify!(#name).to_lowercase())),
                        syntax: Some(String::from("proto3")),
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
                }).clone()
            }

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
                Some(#tags_encoded_len #(+ #field_message_lengths)*)
            }
        }
    };

    TokenStream::from(expanded)
}
