extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};
use proc_macro2;

fn impl_network_delta(fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>) -> proc_macro2::TokenStream {
    let get_delta_bitmask = fields.iter().map(|f| {
        let name = f.ident.as_ref().unwrap();
        let delta_name = syn::Ident::new(&format!("delta_{}", name), name.span()); 

        quote! {
            let (#name, #delta_name) = self.#name.get_delta_bitmask(&self.entities_id, &snapshot.#name, &snapshot.entities_id);
        }
    });

    let fields_delta_name = fields.iter().map(|f| {
        let name = f.ident.as_ref().unwrap();
        let delta_name = syn::Ident::new(&format!("delta_{}", name), name.span()); 

        quote! {
            #name,
            #delta_name,
        }
    });

    let apply_delta_bitmask = fields.iter().map(|f| {
        let name = f.ident.as_ref().unwrap();
        let delta_name = syn::Ident::new(&format!("delta_{}", name), name.span()); 

        quote! {
            let mut #name = self.#name.apply_delta_bitmask(&self.entities_id, &delta.#delta_name, &delta.entities_id);
            #name.join(&delta.#name);
        }
    });

    let fields_name = fields.iter().map(|f| {
        let name = f.ident.as_ref().unwrap();

        quote! { #name }
    });

    let expanded = quote! {
        impl ::netcarrier::Delta for NetworkPacket {
            type DeltaType = NetworkDeltaPacket;

            fn from(&self, snapshot: &Self) -> Option<Self::DeltaType> {
                #(#get_delta_bitmask)*

                Some(NetworkDeltaPacket {
                    frame: self.frame(),
                    snapshot_frame: snapshot.frame(),
                    entities_id: self.entities_id.clone(),
                    #(#fields_delta_name)*
                })
            }

            fn apply(&self, delta: &Self::DeltaType) -> Self {
                #(#apply_delta_bitmask)*

                NetworkPacket {
                    frame: delta.frame,
                    entities_id: delta.entities_id.clone(),
                    #(#fields_name,)*
                }
            }
        }
    };

    expanded
}

#[proc_macro]
pub fn generate_packet(input: TokenStream) -> TokenStream {
    // println!("{:#?}", input);
    let ast = parse_macro_input!(input as DeriveInput);
    // println!("{:#?}", ast);

    let fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma> = if let syn::Data::Struct(syn::DataStruct {
        fields: syn::Fields::Named(syn::FieldsNamed { ref named, .. }),
        ..
    }) = ast.data
    {
        named
    } else {
        unimplemented!();
    };

    let fields_type = fields.iter().map(|f| {
        let name = &f.ident;
        let ty = &f.ty;
        quote! { #name: ::netcarrier::NetworkBitmask<#ty> }
    });

    let fields_type_clone = fields_type.clone();

    let delta_fields_type = fields.iter().map(|f| {
        let name = f.ident.as_ref().unwrap();
        let delta_name = format!("delta_{}", name);
        let delta_ident = syn::Ident::new(&delta_name, name.span()); 
        let ty = &f.ty;
        quote! { #delta_ident: ::netcarrier::NetworkBitmask<<#ty as ::netcarrier::Delta>::DeltaType> }
    });

    let fields_initialized = fields.iter().map(|f| {
		let name = &f.ident;
		let ty = &f.ty;

        quote! { #name: ::netcarrier::replicate::<#ty>(&world, &entities_id) }
    });
    
    let field_apply_state = fields.iter().map(|f| {
		let name = &f.ident;
		let ty = &f.ty;

        quote! {{
			let mut #name = all_storages.borrow::<::netcarrier::shipyard::ViewMut<#ty>>();
			let masked_entities_ids = self.#name.masked_entities_id(&self.entities_id);
			for (i, component) in self.#name.values.iter().enumerate() {
				let net_id = &masked_entities_ids[i];
				if let Some(&id) = net_id_mapping.0.get(net_id) {
					if !#name.contains(id) {
							entities.add_component(&mut #name, *component, id);
					} else {
						#name[id] = *component;
					}
				}
			}
		}}
    });
    
    let impl_network_delta = impl_network_delta(fields);

    let expanded = quote! {
        use ::netcarrier::shipyard::*;
        use ::netcarrier::CarrierPacket;

        #[derive(::netcarrier::serde::Serialize, ::netcarrier::serde::Deserialize, PartialEq, Debug, Clone)]
        pub struct NetworkPacket {
            frame: u32,
            entities_id: Vec<u32>,
            #(#fields_type,)*
		}
		
		#[derive(::netcarrier::serde::Serialize, ::netcarrier::serde::Deserialize, PartialEq, Debug, Clone)]
        pub struct NetworkDeltaPacket {
            frame: u32,
            snapshot_frame: u32,
            entities_id: Vec<u32>,
            #(#fields_type_clone,)*
            #(#delta_fields_type,)*
        }

        impl ::netcarrier::CarrierPacket for NetworkPacket {
            fn frame(&self) -> u32 {
                self.frame
            }

            fn new(world: &::netcarrier::shipyard::World, frame: u32) -> Self {
				let mut entities_id = vec![];
				world.run(|net_ids: ::netcarrier::shipyard::View<::netcarrier::NetworkIdentifier>| {
					for net_id in net_ids.iter() {
						entities_id.push(net_id.id);
					}
				});
				
				NetworkPacket {
                    frame,
                    entities_id: entities_id.clone(),
                    #(#fields_initialized,)*
                }
            }

            fn apply_state(&self, world: &::netcarrier::shipyard::World) {
				world.run(|mut all_storages: ::netcarrier::shipyard::AllStoragesViewMut| {
					let mut removed_entities: Vec<::netcarrier::shipyard::EntityId> = vec![];
					{
						let mut entities = all_storages.borrow::<::netcarrier::shipyard::EntitiesViewMut>();
						let mut net_id_mapping = all_storages.borrow::<::netcarrier::shipyard::UniqueViewMut<::netcarrier::transport::NetworkIdMapping>>();
						// Create new ids
						for entity_id in &self.entities_id {
							if !net_id_mapping.0.contains_key(&entity_id) {
								let entity = entities.add_entity((), ());
								net_id_mapping.0.insert(*entity_id, entity);
							}
						}

						//Remove entities
						for net_id in net_id_mapping.0.keys() {
							if !self.entities_id.contains(net_id) {
								removed_entities.push(net_id_mapping.0[net_id]);
							}
						}

						#(#field_apply_state)*
					}
					for entity_id in removed_entities {
						all_storages.delete(entity_id);
					}
				});
            }
        }

        impl ::netcarrier::CarrierDeltaPacket for NetworkDeltaPacket {
            fn frame(&self) -> u32 {
                self.frame
            }

            fn snapshot_frame(&self) -> u32 {
                self.snapshot_frame
            }
        }

        #impl_network_delta

    };
    expanded.into()
}

