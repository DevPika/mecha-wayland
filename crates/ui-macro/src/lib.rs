use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Field, Fields, Ident, ItemStruct, Token, Type, parse::Parse, parse::ParseStream,
    parse_macro_input, parse_quote, punctuated::Punctuated,
};

enum WidgetArg {
    None,
    Measure,
}

impl Parse for WidgetArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(WidgetArg::None);
        }
        let ident: Ident = input.parse()?;
        match ident.to_string().as_str() {
            "measure" => Ok(WidgetArg::Measure),
            other => Err(syn::Error::new(
                ident.span(),
                format!("unknown #[widget] arg `{other}`; expected `measure`"),
            )),
        }
    }
}

/// Transforms a named struct into a widget.
///
/// Injects private `node_id: taffy::NodeId`, `style: taffy::Style`,
/// `bounds: utils::Rect`, `pending_damage: ui::Damage`, and a public
/// `is_opaque: bool` (opt-out of the opaque fast path; init to `true`) fields,
/// then generates
/// `impl Widget` (including `drain_damage`), an `impl OnChange<Style>`, and the
/// inherent `set::<T>` method. The `bounds` field caches the widget's current
/// **absolute** on-screen rect; the render walk writes it (it is the only pass
/// that accumulates offset from the root). `pending_damage` accumulates the
/// damage of each `set` until `drain_damage` collects it. Like `node_id`, the
/// author must initialize both in the constructor (`bounds: utils::Rect::ZERO`,
/// `pending_damage: ui::Damage::None`) — the macro only injects the fields, it
/// does not generate the constructor.
///
/// - `#[widget]`            — leaf widget (no measure, no children)
/// - `#[widget(measure)]`   — leaf widget; user must `impl Measure for Foo`; `Foo` must be `Clone + 'static`
/// - `#[widget(child)]`     — annotate a field whose type implements `WidgetList`
///
/// `#[widget(measure)]` and `#[widget(child)]` are mutually exclusive.
#[proc_macro_attribute]
pub fn widget(attr: TokenStream, input: TokenStream) -> TokenStream {
    let arg = parse_macro_input!(attr as WidgetArg);
    let mut item = parse_macro_input!(input as ItemStruct);

    let named = match &mut item.fields {
        Fields::Named(f) => &mut f.named,
        _ => return quote!(compile_error!("#[widget] only supports named structs");).into(),
    };

    // Collect #[widget(child)] field names before stripping attrs.
    let child_fields: Vec<Ident> = named
        .iter()
        .filter(|f| is_widget_child(f))
        .map(|f| f.ident.clone().unwrap())
        .collect();

    if matches!(arg, WidgetArg::Measure) && !child_fields.is_empty() {
        return quote!(compile_error!("#[widget(measure)] and #[widget(child)] are mutually exclusive");).into();
    }

    // Strip #[widget(...)] attrs from every field so they don't appear in output.
    for f in named.iter_mut() {
        f.attrs.retain(|a| !a.path().is_ident("widget"));
    }

    let node_id_field: Field = parse_quote!(node_id: ::taffy::NodeId);
    let style_field: Field = parse_quote!(style: ::taffy::Style);
    let bounds_field: Field = parse_quote!(bounds: ::utils::Rect);
    let pending_damage_field: Field = parse_quote!(pending_damage: ::ui::Damage);
    let is_opaque_field: Field = parse_quote!(pub is_opaque: bool);
    named.insert(0, is_opaque_field);
    named.insert(0, pending_damage_field);
    named.insert(0, bounds_field);
    named.insert(0, style_field);
    named.insert(0, node_id_field);

    let name = &item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let is_measure = matches!(arg, WidgetArg::Measure);
    let build_tree_body = build_tree_body(&child_fields, is_measure);
    let render_node_body = render_node_body(&child_fields);
    let on_event_body = on_event_body(&child_fields);
    let drain_damage_body = drain_damage_body(&child_fields, is_measure);

    quote! {
        #item

        impl #impl_generics ::ui::Widget for #name #ty_generics #where_clause {
            fn node_id(&self) -> ::taffy::NodeId { self.node_id }
            fn style(&self) -> &::taffy::Style { &self.style }
            fn own_bounds(&self) -> ::utils::Rect { self.bounds }

            fn build_tree(&mut self, tree: &mut ::ui::WidgetTree) -> ::taffy::NodeId {
                #build_tree_body
            }

            fn render_node(
                &mut self,
                layout: &::taffy::Layout,
                tree: &::ui::WidgetTree,
                offset: ::ui::Point,
                z: f32,
                background: ::utils::Color,
            ) -> ::std::vec::Vec<::ui::RenderCommand> {
                #render_node_body
            }

            fn on_event(
                &mut self,
                ctx: &mut ::ui::EventCtx,
            ) {
                #on_event_body
            }

            fn drain_damage(&mut self, tree: &mut ::ui::WidgetTree) -> ::ui::Damage {
                #drain_damage_body
            }
        }

        impl #impl_generics ::ui::OnChange<::taffy::Style> for #name #ty_generics #where_clause {
            fn damage(&self, _new: &::taffy::Style) -> ::ui::Damage {
                ::ui::Damage::Layout
            }
            fn change(&mut self, new: ::taffy::Style) {
                self.style = new;
            }
        }

        impl #impl_generics #name #ty_generics #where_clause {
            pub fn set<__V>(&mut self, value: __V)
            where
                Self: ::ui::OnChange<__V>,
            {
                self.pending_damage |= <Self as ::ui::OnChange<__V>>::damage(self, &value);
                <Self as ::ui::OnChange<__V>>::change(self, value);
            }
        }
    }
    .into()
}

fn is_widget_child(field: &Field) -> bool {
    field.attrs.iter().any(|a| {
        a.path().is_ident("widget") && a.parse_args::<Ident>().map_or(false, |i| i == "child")
    })
}

fn build_tree_body(child_fields: &[Ident], is_measure: bool) -> TokenStream2 {
    if !child_fields.is_empty() {
        quote! {
            let mut __ids: ::std::vec::Vec<::taffy::NodeId> = ::std::vec::Vec::new();
            #(
                __ids.extend(::ui::WidgetList::build_children(&mut self.#child_fields, tree));
            )*
            let id = tree.new_with_children(self.style.clone(), &__ids).unwrap();
            self.node_id = id;
            id
        }
    } else if is_measure {
        quote! {
            let context = ::std::boxed::Box::new(self.clone()) as ::std::boxed::Box<dyn ::ui::Measure>;
            let id = tree.new_leaf_with_context(self.style.clone(), context).unwrap();
            self.node_id = id;
            id
        }
    } else {
        quote! {
            let id = tree.new_leaf(self.style.clone()).unwrap();
            self.node_id = id;
            id
        }
    }
}

fn render_node_body(child_fields: &[Ident]) -> TokenStream2 {
    let own = quote! {
        let __abs = ::ui::Point::new(offset.x() + layout.location.x, offset.y() + layout.location.y);
        self.bounds = ::utils::Rect {
            origin: __abs,
            size: ::utils::Size::new(layout.size.width, layout.size.height),
        };
        let mut __cmds = ::ui::Render::render(self, layout, __abs);
        for __cmd in &mut __cmds {
            __cmd.stamp(z, background, self.is_opaque);
        }
    };

    if child_fields.is_empty() {
        quote! {
            #own
            __cmds
        }
    } else {
        quote! {
            #own
            // Children descend one z-step deeper, over this node's fill.
            let __child_z = z + ::ui::Z_STEP;
            let __child_bg = ::ui::Render::fill(self).over(background);
            #(
                __cmds.extend(::ui::WidgetList::render_children(
                    &mut self.#child_fields, tree, __abs, __child_z, __child_bg,
                ));
            )*
            __cmds
        }
    }
}

fn on_event_body(child_fields: &[Ident]) -> TokenStream2 {
    if child_fields.is_empty() {
        quote! {}
    } else {
        quote! {
            #(
                ::ui::WidgetList::on_event(&mut self.#child_fields, ctx);
            )*
        }
    }
}

fn drain_damage_body(child_fields: &[Ident], is_measure: bool) -> TokenStream2 {
    let reconcile = if is_measure {
        quote! {
            tree.set_style(self.node_id, self.style.clone()).unwrap();
            let __context = ::std::boxed::Box::new(self.clone()) as ::std::boxed::Box<dyn ::ui::Measure>;
            tree.set_node_context(self.node_id, ::std::option::Option::Some(__context)).unwrap();
            tree.mark_dirty(self.node_id).unwrap();
        }
    } else {
        quote! {
            tree.set_style(self.node_id, self.style.clone()).unwrap();
            tree.mark_dirty(self.node_id).unwrap();
        }
    };

    quote! {
        let mut __d = ::std::mem::take(&mut self.pending_damage);
        if ::std::matches!(__d, ::ui::Damage::Layout) {
            #reconcile
        }
        #(
            __d |= ::ui::WidgetList::flush_damage(&mut self.#child_fields, tree);
        )*
        __d
    }
}

/// Generates a `Module<WindowManager>` that drains `WindowManager::event_buffer`
/// on each `UiEventsReady` event and re-dispatches the listed event types.
///
/// Usage: `register_events!(TypeA, TypeB, ...)`
///
/// Mount it alongside `window_manager::module()`:
/// ```ignore
/// App::new(state)
///     .mount(window_manager::module())
///     .mount(register_events!(MyEvent))
/// ```
#[proc_macro]
pub fn register_events(input: TokenStream) -> TokenStream {
    let event_types =
        parse_macro_input!(input with Punctuated::<Type, Token![,]>::parse_terminated);
    let event_types: Vec<_> = event_types.into_iter().collect();

    let extract_handlers = event_types.iter().map(|ty| {
        quote! {
            let __m = __m.on(
                |__wm: &mut ::window_manager::WindowManager,
                 _: &::window_manager::UiEventsReady| {
                    let __buf = ::std::mem::take(&mut __wm.event_buffer);
                    let mut __vec: ::std::vec::Vec<#ty> = ::std::vec::Vec::new();
                    for __e in __buf {
                        match __e.downcast::<#ty>() {
                            ::std::result::Result::Ok(__v) => __vec.push(*__v),
                            ::std::result::Result::Err(__e) => __wm.event_buffer.push(__e),
                        }
                    }
                    ::app::Many(__vec)
                },
            );
        }
    });

    quote! {
        {
            // Type inferred from the first .on() call below.
            let __m = ::app::Module::new();
            // let __m = __m.on(
            //     |__wm: &mut ::window_manager::WindowManager,
            //      _: &::window_manager::UiEventsReady| {
            //         __wm.event_buffer.clear();
            //     },
            // );
            #(#extract_handlers)*
            __m
        }
    }
    .into()
}
