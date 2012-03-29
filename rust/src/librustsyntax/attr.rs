// Functions dealing with attributes and meta_items

import std::map;
import std::map::hashmap;
import either::either;
import diagnostic::span_handler;

export attr_meta;
export attr_metas;
export find_linkage_metas;
export inline_attr;
export find_inline_attr;
export find_attrs_by_name;
export attrs_contains_name;
export find_meta_items_by_name;
export contains;
export contains_name;
export sort_meta_items;
export remove_meta_items_by_name;
export require_unique_names;
export get_attr_name;
export get_meta_item_name;
export get_meta_item_value_str;
export get_meta_item_value_str_by_name;
export get_meta_item_list;
export meta_item_value_from_list;
export meta_item_list_from_list;
export name_value_str_pair;
export mk_name_value_item_str;
export mk_name_value_item;
export mk_list_item;
export mk_word_item;
export mk_attr;
export native_abi;

// From a list of crate attributes get only the meta_items that impact crate
// linkage
fn find_linkage_metas(attrs: [ast::attribute]) -> [@ast::meta_item] {
    let mut metas: [@ast::meta_item] = [];
    for attr: ast::attribute in find_attrs_by_name(attrs, "link") {
        alt attr.node.value.node {
          ast::meta_list(_, items) { metas += items; }
          _ { #debug("ignoring link attribute that has incorrect type"); }
        }
    }
    ret metas;
}

enum inline_attr {
    ia_none,
    ia_hint,
    ia_always
}

// True if something like #[inline] is found in the list of attrs.
fn find_inline_attr(attrs: [ast::attribute]) -> inline_attr {
    // TODO---validate the usage of #[inline] and #[inline(always)]
    vec::foldl(ia_none, attrs) {|ia,attr|
        alt attr.node.value.node {
          ast::meta_word("inline") { ia_hint }
          ast::meta_list("inline", items) {
            if !vec::is_empty(find_meta_items_by_name(items, "always")) {
                ia_always
            } else {
                ia_hint
            }
          }
          _ { ia }
        }
    }
}

// Search a list of attributes and return only those with a specific name
fn find_attrs_by_name(attrs: [ast::attribute], name: ast::ident) ->
   [ast::attribute] {
    let filter = (
        fn@(a: ast::attribute) -> option<ast::attribute> {
            if get_attr_name(a) == name {
                option::some(a)
            } else { option::none }
        }
    );
    ret vec::filter_map(attrs, filter);
}

fn attrs_contains_name(attrs: [ast::attribute], name: ast::ident) -> bool {
    vec::is_not_empty(find_attrs_by_name(attrs, name))
}

fn get_attr_name(attr: ast::attribute) -> ast::ident {
    get_meta_item_name(@attr.node.value)
}

fn find_meta_items_by_name(metas: [@ast::meta_item], name: ast::ident) ->
   [@ast::meta_item] {
    let filter = fn@(&&m: @ast::meta_item) -> option<@ast::meta_item> {
        if get_meta_item_name(m) == name {
            option::some(m)
        } else { option::none }
    };
    ret vec::filter_map(metas, filter);
}

fn get_meta_item_name(meta: @ast::meta_item) -> ast::ident {
    alt meta.node {
      ast::meta_word(n) { n }
      ast::meta_name_value(n, _) { n }
      ast::meta_list(n, _) { n }
    }
}

// Gets the string value if the meta_item is a meta_name_value variant
// containing a string, otherwise none
fn get_meta_item_value_str(meta: @ast::meta_item) -> option<str> {
    alt meta.node {
      ast::meta_name_value(_, v) {
        alt v.node { ast::lit_str(s) { option::some(s) } _ { option::none } }
      }
      _ { option::none }
    }
}

fn get_meta_item_value_str_by_name(attrs: [ast::attribute], name: ast::ident)
    -> option<str> {
    let mattrs = find_attrs_by_name(attrs, name);
    if vec::len(mattrs) > 0u {
        ret get_meta_item_value_str(attr_meta(mattrs[0]));
    }
    ret option::none;
}

fn get_meta_item_list(meta: @ast::meta_item) -> option<[@ast::meta_item]> {
    alt meta.node {
      ast::meta_list(_, l) { option::some(l) }
      _ { option::none }
    }
}

fn attr_meta(attr: ast::attribute) -> @ast::meta_item { @attr.node.value }

// Get the meta_items from inside a vector of attributes
fn attr_metas(attrs: [ast::attribute]) -> [@ast::meta_item] {
    let mut mitems = [];
    for a: ast::attribute in attrs { mitems += [attr_meta(a)]; }
    ret mitems;
}

fn eq(a: @ast::meta_item, b: @ast::meta_item) -> bool {
    ret alt a.node {
          ast::meta_word(na) {
            alt b.node { ast::meta_word(nb) { na == nb } _ { false } }
          }
          ast::meta_name_value(na, va) {
            alt b.node {
              ast::meta_name_value(nb, vb) { na == nb && va.node == vb.node }
              _ { false }
            }
          }
          ast::meta_list(na, la) {

            // FIXME (#607): Needs implementing
            // This involves probably sorting the list by name and
            // meta_item variant
            fail "unimplemented meta_item variant"
          }
        }
}

fn contains(haystack: [@ast::meta_item], needle: @ast::meta_item) -> bool {
    #debug("looking for %s",
           print::pprust::meta_item_to_str(*needle));
    for item: @ast::meta_item in haystack {
        #debug("looking in %s",
               print::pprust::meta_item_to_str(*item));
        if eq(item, needle) { #debug("found it!"); ret true; }
    }
    #debug("found it not :(");
    ret false;
}

fn contains_name(metas: [@ast::meta_item], name: ast::ident) -> bool {
    let matches = find_meta_items_by_name(metas, name);
    ret vec::len(matches) > 0u;
}

// FIXME: This needs to sort by meta_item variant in addition to the item name
fn sort_meta_items(items: [@ast::meta_item]) -> [@ast::meta_item] {
    fn lteq(&&ma: @ast::meta_item, &&mb: @ast::meta_item) -> bool {
        fn key(m: @ast::meta_item) -> ast::ident {
            alt m.node {
              ast::meta_word(name) { name }
              ast::meta_name_value(name, _) { name }
              ast::meta_list(name, _) { name }
            }
        }
        ret key(ma) <= key(mb);
    }

    // This is sort of stupid here, converting to a vec of mutables and back
    let mut v: [mut @ast::meta_item] = [mut];
    for mi: @ast::meta_item in items { v += [mut mi]; }

    std::sort::quick_sort(lteq, v);

    let mut v2: [@ast::meta_item] = [];
    for mi: @ast::meta_item in v { v2 += [mi]; }
    ret v2;
}

fn remove_meta_items_by_name(items: [@ast::meta_item], name: str) ->
   [@ast::meta_item] {

    let filter = fn@(&&item: @ast::meta_item) -> option<@ast::meta_item> {
        if get_meta_item_name(item) != name {
            option::some(item)
        } else { option::none }
    };

    ret vec::filter_map(items, filter);
}

fn require_unique_names(diagnostic: span_handler,
                        metas: [@ast::meta_item]) {
    let map = map::str_hash();
    for meta: @ast::meta_item in metas {
        let name = get_meta_item_name(meta);
        if map.contains_key(name) {
            diagnostic.span_fatal(meta.span,
                                  #fmt["duplicate meta item `%s`", name]);
        }
        map.insert(name, ());
    }
}

fn native_abi(attrs: [ast::attribute]) -> either<str, ast::native_abi> {
    ret alt attr::get_meta_item_value_str_by_name(attrs, "abi") {
      option::none {
        either::right(ast::native_abi_cdecl)
      }
      option::some("rust-intrinsic") {
        either::right(ast::native_abi_rust_intrinsic)
      }
      option::some("cdecl") {
        either::right(ast::native_abi_cdecl)
      }
      option::some("stdcall") {
        either::right(ast::native_abi_stdcall)
      }
      option::some(t) {
        either::left("unsupported abi: " + t)
      }
    };
}

fn meta_item_from_list(
    items: [@ast::meta_item],
    name: str
) -> option<@ast::meta_item> {
    let items = attr::find_meta_items_by_name(items, name);
    vec::last_opt(items)
}

fn meta_item_value_from_list(
    items: [@ast::meta_item],
    name: str
) -> option<str> {
    alt meta_item_from_list(items, name) {
      some(item) {
        alt attr::get_meta_item_value_str(item) {
          some(value) { some(value) }
          none { none }
        }
      }
      none { none }
    }
}

fn meta_item_list_from_list(
    items: [@ast::meta_item],
    name: str
) -> option<[@ast::meta_item]> {
    alt meta_item_from_list(items, name) {
      some(item) {
        attr::get_meta_item_list(item)
      }
      none { none }
    }
}

fn name_value_str_pair(
    item: @ast::meta_item
) -> option<(str, str)> {
    alt attr::get_meta_item_value_str(item) {
      some(value) {
        let name = attr::get_meta_item_name(item);
        some((name, value))
      }
      none { none }
    }
}

fn span<T: copy>(item: T) -> ast::spanned<T> {
    ret {node: item, span: ast_util::dummy_sp()};
}

fn mk_name_value_item_str(name: ast::ident, value: str) -> @ast::meta_item {
    let value_lit = span(ast::lit_str(value));
    ret mk_name_value_item(name, value_lit);
}

fn mk_name_value_item(name: ast::ident, value: ast::lit) -> @ast::meta_item {
    ret @span(ast::meta_name_value(name, value));
}

fn mk_list_item(name: ast::ident, items: [@ast::meta_item]) ->
   @ast::meta_item {
    ret @span(ast::meta_list(name, items));
}

fn mk_word_item(name: ast::ident) -> @ast::meta_item {
    ret @span(ast::meta_word(name));
}

fn mk_attr(item: @ast::meta_item) -> ast::attribute {
    ret span({style: ast::attr_inner, value: *item});
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
