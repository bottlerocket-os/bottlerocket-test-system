use proc_macro2::{Ident, Span, TokenStream};
use syn::{self, Data, LitStr};

extern crate quote;

pub(crate) fn builder_fn(ast: &syn::DeriveInput) -> TokenStream {
    let builder_name = format!("{}{}", &ast.ident.to_string(), "Builder");
    let builder_ident = Ident::new(&builder_name, Span::call_site());
    let ident = &ast.ident;
    quote! {
       impl #ident{
           pub fn builder() -> #builder_ident {
               #builder_ident::new()
           }
       }
    }
}

pub(crate) fn build_struct(ast: &syn::DeriveInput) -> TokenStream {
    let name = format!("{}{}", &ast.ident.to_string(), "Builder");
    let build_ident = Ident::new(&name, Span::call_site());
    let data = match &ast.data {
        Data::Struct(data) => data,
        _ => panic!("configuration-derive only supports structs"),
    };
    let crd_type = ast
        .attrs
        .iter()
        .filter_map(|v| {
            v.parse_meta().ok().map(|meta| {
                if meta.path().is_ident("crd") {
                    v.parse_args::<LitStr>().ok()
                } else {
                    None
                }
            })
        })
        .last()
        .flatten()
        .expect("`crd` is a required attribute (Test, Resource)")
        .value();

    // Get a list of fields and their types
    let fields = data.fields.iter().filter_map(|field| {
        let attrs = field.attrs.iter().filter(|v| {
            v.parse_meta()
                .map(|meta| meta.path().is_ident("doc") || meta.path().is_ident("serde"))
                .unwrap_or(false)
        });
        let field_name = match field.ident.as_ref() {
            Some(ident) => ident.to_string(),
            None => return None,
        };
        let field_ident = Ident::new(&field_name, Span::call_site());
        let ty = field.ty.clone();
        Some(quote! {
            #(#attrs)*
            #field_ident: model::ConfigValue<#ty>,
        })
    });
    // Create the setters for each field, one for typed values and one for templated strings
    let setters = data.fields.iter().filter_map(|field| {
        let doc = field.attrs.iter().filter(|v| {
            v.parse_meta()
                .map(|meta| meta.path().is_ident("doc"))
                .unwrap_or(false)
        });
        let field_name = match field.ident.as_ref() {
            Some(ident) => ident.to_string(),
            None => return None,
        };
        let field_ident = Ident::new(&field_name, Span::call_site());
        let template_field_name = format!("{}{}", field_name, "_template");
        let template_ident = Ident::new(&template_field_name, Span::call_site());
        let ty = field.ty.clone();
        Some(quote! {
            #(#doc)*
            #[inline(always)]
            pub fn #field_ident<T>(&mut self, #field_ident: T) -> &mut Self
            where
            T: Into<#ty>{
                self.#field_ident = model::ConfigValue::Value(#field_ident.into());
                self
            }

            #[inline(always)]
            pub fn #template_ident<S1,S2>(&mut self, resource: S1, field: S2) -> &mut Self
            where
            S1: Into<String>,
            S2: Into<String>,
            {
                self.#field_ident = model::ConfigValue::TemplatedString(format!("${{{}.{}}}", resource.into(), field.into()));
                self
            }
        })
    });
    let fns = quote! {
        #(#setters)*
    };

    // Add the build function to the builders.
    let build = match crd_type.as_str() {
        "Test" => {
            quote! {
                #[derive(Debug, Default, serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                pub struct #build_ident{
                    #(#fields)*
                    depends_on: Vec<String>,
                    resources: Vec<String>,
                    labels: std::collections::BTreeMap<String,String>,
                    image: Option<String>,
                    image_pull_secret: Option<String>,
                    secrets: std::collections::BTreeMap<String, model::SecretName>,
                    retries: Option<u32>,
                    keep_running: Option<bool>,
                    capabilities: Vec<String>,
                    privileged: Option<bool>,
                }

                impl #build_ident{
                    pub fn new() -> #build_ident {
                        Default::default()
                    }

                    #fns

                    pub fn depends_on<S1>(&mut self, depends_on: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.depends_on.push(depends_on.into());
                        self
                    }

                    pub fn set_depends_on(&mut self, depends_on: Option<Vec<String>>) -> &mut Self {
                        self.depends_on = depends_on.unwrap_or_default();
                        self
                    }

                    pub fn resources<S1>(&mut self, resources: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.resources.push(resources.into());
                        self
                    }

                    pub fn set_resources(&mut self, resources: Option<Vec<String>>) -> &mut Self {
                        self.resources = resources.unwrap_or_default();
                        self
                    }

                    pub fn labels<S1, S2>(&mut self, key: S1, value: S2) -> &mut Self
                    where
                    S1: Into<String>,
                    S2: Into<String> {
                        self.labels.insert(key.into(), value.into());
                        self
                    }

                    pub fn set_labels(&mut self, labels: Option<std::collections::BTreeMap<String,String>>) -> &mut Self {
                        self.labels = labels.unwrap_or_default();
                        self
                    }

                    pub fn image<S1>(&mut self, image: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.image = Some(image.into());
                        self
                    }

                    pub fn set_image(&mut self, image: Option<String>) -> &mut Self {
                        self.image = image;
                        self
                    }

                    pub fn image_pull_secret<S1>(&mut self, image_pull_secret: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.image_pull_secret = Some(image_pull_secret.into());
                        self
                    }

                    pub fn set_image_pull_secret(&mut self, image_pull_secret: Option<String>) -> &mut Self {
                        self.image_pull_secret = image_pull_secret;
                        self
                    }

                    pub fn secrets<S1, S2>(&mut self, key: S1, value: model::SecretName) -> &mut Self
                    where
                    S1: Into<String>{
                        self.secrets.insert(key.into(), value.into());
                        self
                    }

                    pub fn set_secrets(&mut self, secrets: Option<std::collections::BTreeMap<String,model::SecretName>>) -> &mut Self {
                        self.secrets = secrets.unwrap_or_default();
                        self
                    }

                    pub fn retries<S1>(&mut self, retries: u32) -> &mut Self {
                        self.retries = Some(retries);
                        self
                    }

                    pub fn set_retries(&mut self, retries: Option<u32>) -> &mut Self {
                        self.retries = retries;
                        self
                    }

                    pub fn keep_running(&mut self, keep_running: bool) -> &mut Self {
                        self.keep_running = Some(keep_running);
                        self
                    }

                    pub fn set_keep_running(&mut self, keep_running: Option<bool>) -> &mut Self {
                        self.keep_running = keep_running;
                        self
                    }

                    pub fn capabilities<S1>(&mut self, capabilities: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.capabilities.push(capabilities.into());
                        self
                    }

                    pub fn set_capabilities(&mut self, capabilities: Option<Vec<String>>) -> &mut Self {
                        self.capabilities = capabilities.unwrap_or_default();
                        self
                    }

                    pub fn privileged(&mut self, privileged: bool) -> &mut Self {
                        self.privileged = Some(privileged);
                        self
                    }

                    pub fn set_privileged(&mut self, privileged: Option<bool>) -> &mut Self {
                        self.privileged = privileged;
                        self
                    }

                    pub fn build<S1>(&self, name: S1) -> Result<model::Test, Box<dyn std::error::Error>>
                    where
                    S1: Into<String>,
                    {
                        let configuration =  match serde_json::to_value(self) {
                            Ok(serde_json::Value::Object(map)) => map,
                            Err(error) => return Err(format!("Unable to serialize config: {}", error).into()),
                            _ => return Err("Configuration must be a map".into()),
                        };

                        Ok(model::create_test_crd(name, Some(&self.labels), model::TestSpec {
                                resources: self.resources.clone(),
                                depends_on: Some(self.depends_on.clone()),
                                retries: Some(self.retries.as_ref().cloned().unwrap_or(5)),
                                agent: model::Agent {
                                    name: "agent".to_string(),
                                    image: self.image.as_ref().cloned().ok_or_else(|| "Image is required to build a test".to_string())?,
                                    pull_secret: self.image_pull_secret.as_ref().cloned(),
                                    keep_running: self.keep_running.as_ref().cloned().unwrap_or(true),
                                    configuration: Some(configuration),
                                    secrets: Some(self.secrets.clone()),
                                    capabilities: Some(self.capabilities.clone()),
                                    privileged: self.privileged,
                                    timeout: None
                                },
                            },
                        ))
                    }

                }
            }
        }
        "Resource" => {
            quote! {
                #[derive(Debug, Default, serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                pub struct #build_ident{
                    #(#fields)*
                    depends_on: Vec<String>,
                    conflicts_with: Vec<String>,
                    labels: std::collections::BTreeMap<String,String>,
                    image: Option<String>,
                    image_pull_secret: Option<String>,
                    secrets: std::collections::BTreeMap<String, model::SecretName>,
                    keep_running: Option<bool>,
                    capabilities: Vec<String>,
                    destruction_policy: Option<model::DestructionPolicy>,
                    privileged: Option<bool>,
                }

                impl #build_ident{
                    pub fn new() -> #build_ident {
                        Default::default()
                    }

                    #fns

                    pub fn depends_on<S1>(&mut self, depends_on: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.depends_on.push(depends_on.into());
                        self
                    }

                    pub fn set_depends_on(&mut self, depends_on: Option<Vec<String>>) -> &mut Self {
                        self.depends_on = depends_on.unwrap_or_default();
                        self
                    }

                    pub fn conflicts_with<S1>(&mut self, conflicts_with: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.conflicts_with.push(conflicts_with.into());
                        self
                    }

                    pub fn set_conflicts_with(&mut self, conflicts_with: Option<Vec<String>>) -> &mut Self {
                        self.conflicts_with = conflicts_with.unwrap_or_default();
                        self
                    }

                    pub fn labels<S1, S2>(&mut self, key: S1, value: S2) -> &mut Self
                    where
                    S1: Into<String>,
                    S2: Into<String> {
                        self.labels.insert(key.into(), value.into());
                        self
                    }

                    pub fn set_labels(&mut self, labels: Option<std::collections::BTreeMap<String,String>>) -> &mut Self {
                        self.labels = labels.unwrap_or_default();
                        self
                    }

                    pub fn image<S1>(&mut self, image: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.image = Some(image.into());
                        self
                    }

                    pub fn set_image(&mut self, image: Option<String>) -> &mut Self {
                        self.image = image;
                        self
                    }

                    pub fn image_pull_secret<S1>(&mut self, image_pull_secret: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.image_pull_secret = Some(image_pull_secret.into());
                        self
                    }

                    pub fn set_image_pull_secret(&mut self, image_pull_secret: Option<String>) -> &mut Self {
                        self.image_pull_secret = image_pull_secret;
                        self
                    }

                    pub fn secrets<S1, S2>(&mut self, key: S1, value: model::SecretName) -> &mut Self
                    where
                    S1: Into<String>{
                        self.secrets.insert(key.into(), value.into());
                        self
                    }

                    pub fn set_secrets(&mut self, secrets: Option<std::collections::BTreeMap<String,model::SecretName>>) -> &mut Self {
                        self.secrets = secrets.unwrap_or_default();
                        self
                    }

                    pub fn keep_running(&mut self, keep_running: bool) -> &mut Self {
                        self.keep_running = Some(keep_running);
                        self
                    }

                    pub fn set_keep_running(&mut self, keep_running: Option<bool>) -> &mut Self {
                        self.keep_running = keep_running;
                        self
                    }

                    pub fn capabilities<S1>(&mut self, capabilities: S1) -> &mut Self
                    where
                    S1: Into<String> {
                        self.capabilities.push(capabilities.into());
                        self
                    }

                    pub fn set_capabilities(&mut self, capabilities: Option<Vec<String>>) -> &mut Self {
                        self.capabilities = capabilities.unwrap_or_default();
                        self
                    }

                    pub fn destruction_policy(&mut self, destruction_policy: model::DestructionPolicy) -> &mut Self {
                        self.destruction_policy = Some(destruction_policy);
                        self
                    }

                    pub fn set_destruction_policy(&mut self, destruction_policy: Option<model::DestructionPolicy>) -> &mut Self {
                        self.destruction_policy = destruction_policy;
                        self
                    }

                    pub fn privileged(&mut self, privileged: bool) -> &mut Self {
                        self.privileged = Some(privileged);
                        self
                    }

                    pub fn set_privileged(&mut self, privileged: Option<bool>) -> &mut Self {
                        self.privileged = privileged;
                        self
                    }

                    pub fn build<S1>(&self, name: S1) -> Result<model::Resource, Box<dyn std::error::Error>>
                    where
                    S1: Into<String>,
                    {
                        let configuration =  match serde_json::to_value(self) {
                            Ok(serde_json::Value::Object(map)) => map,
                            Err(error) => return Err(format!("Unable to serialize config: {}", error).into()),
                            _ => return Err("Configuration must be a map".to_string().into()),
                        };

                        Ok(model::create_resource_crd(name, Some(&self.labels), model::ResourceSpec {
                            conflicts_with: Some(self.conflicts_with.clone()),
                            depends_on: Some(self.depends_on.clone()),
                            agent: model::Agent {
                                name: "agent".to_string(),
                                image: self.image.as_ref().cloned().ok_or_else(|| "Image is required to build a test".to_string())?,
                                pull_secret: self.image_pull_secret.as_ref().cloned(),
                                keep_running: self.keep_running.as_ref().cloned().unwrap_or(true),
                                configuration: Some(configuration),
                                secrets: Some(self.secrets.clone()),
                                capabilities: Some(self.capabilities.clone()),
                                timeout: None,
                                privileged: self.privileged,
                            },
                            destruction_policy: self.destruction_policy.as_ref().cloned().unwrap_or_default()
                        },
                        ))
                    }

                }
            }
        }
        _ => panic!(
            "Unexpected crd type '{}'. Crd type must be `Test` or `Resource`",
            crd_type
        ),
    };

    quote! {
        #build
    }
}
