# Copyright 2014 The Rust Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution and at
# http://rust-lang.org/COPYRIGHT.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

import lldb


def print_val(val, internal_dict):
    '''Prints the given value with Rust syntax'''
    type_class = val.GetType().GetTypeClass()

    if type_class == lldb.eTypeClassStruct:
        return print_struct_val(val, internal_dict)

    if type_class == lldb.eTypeClassUnion:
        return print_enum_val(val, internal_dict)

    if type_class == lldb.eTypeClassPointer:
        return print_pointer_val(val, internal_dict)

    if type_class == lldb.eTypeClassArray:
        return print_fixed_size_vec_val(val, internal_dict)

    return val.GetValue()


#=--------------------------------------------------------------------------------------------------
# Type-Specialized Printing Functions
#=--------------------------------------------------------------------------------------------------

def print_struct_val(val, internal_dict):
    '''Prints a struct, tuple, or tuple struct value with Rust syntax'''
    assert val.GetType().GetTypeClass() == lldb.eTypeClassStruct

    if is_vec_slice(val):
        return print_vec_slice_val(val, internal_dict)
    elif is_std_vec(val):
        return print_std_vec_val(val, internal_dict)
    else:
        return print_struct_val_starting_from(0, val, internal_dict)


def print_struct_val_starting_from(field_start_index, val, internal_dict):
    '''
    Prints a struct, tuple, or tuple struct value with Rust syntax.
    Ignores any fields before field_start_index.
    '''
    assert val.GetType().GetTypeClass() == lldb.eTypeClassStruct

    t = val.GetType()
    type_name = extract_type_name(t.GetName())
    num_children = val.num_children

    if (num_children - field_start_index) == 0:
        # The only field of this struct is the enum discriminant
        return type_name

    has_field_names = type_has_field_names(t)

    if has_field_names:
        template = "%(type_name)s {\n%(body)s\n}"
        separator = ", \n"
    else:
        template = "%(type_name)s(%(body)s)"
        separator = ", "

    if type_name.startswith("("):
        # this is a tuple, so don't print the type name
        type_name = ""

    def render_child(child_index):
        this = ""
        if has_field_names:
            field_name = t.GetFieldAtIndex(child_index).GetName()
            this += field_name + ": "

        field_val = val.GetChildAtIndex(child_index)

        if not field_val.IsValid():
            field = t.GetFieldAtIndex(child_index)
            # LLDB is not good at handling zero-sized values, so we have to help
            # it a little
            if field.GetType().GetByteSize() == 0:
                return this + extract_type_name(field.GetType().GetName())
            else:
                return this + "<invalid value>"

        return this + print_val(field_val, internal_dict)

    body = separator.join([render_child(idx) for idx in range(field_start_index, num_children)])

    return template % {"type_name": type_name,
                       "body": body}


def print_enum_val(val, internal_dict):
    '''Prints an enum value with Rust syntax'''

    assert val.GetType().GetTypeClass() == lldb.eTypeClassUnion

    if val.num_children == 1:
        # This is either an enum with just one variant, or it is an Option-like
        # enum where the discriminant is encoded in a non-nullable pointer
        # field. We find out which one it is by looking at the member name of
        # the sole union variant. If it starts with "RUST$ENCODED$ENUM$" then
        # we have an Option-like enum.
        first_variant_name = val.GetChildAtIndex(0).GetName()
        if first_variant_name and first_variant_name.startswith("RUST$ENCODED$ENUM$"):

            # This is an Option-like enum. The position of the discriminator field is
            # encoded in the name which has the format:
            #  RUST$ENCODED$ENUM$<index of discriminator field>$<name of null variant>
            last_separator_index = first_variant_name.rfind("$")
            if last_separator_index == -1:
                return "<invalid enum encoding: %s>" % first_variant_name

            start_index = len("RUST$ENCODED$ENUM$")

            # Extract indices of the discriminator field
            try:
                disr_field_indices = first_variant_name[start_index:last_separator_index].split("$")
                disr_field_indices = [int(index) for index in disr_field_indices]
            except:
                return "<invalid enum encoding: %s>" % first_variant_name

            # Read the discriminant
            disr_val = val.GetChildAtIndex(0)
            for index in disr_field_indices:
                disr_val = disr_val.GetChildAtIndex(index)

            # If the discriminant field is a fat pointer we have to consider the
            # first word as the true discriminant
            if disr_val.GetType().GetTypeClass() == lldb.eTypeClassStruct:
                disr_val = disr_val.GetChildAtIndex(0)

            if disr_val.GetValueAsUnsigned() == 0:
                # Null case: Print the name of the null-variant
                null_variant_name = first_variant_name[last_separator_index + 1:]
                return null_variant_name
            else:
                # Non-null case: Interpret the data as a value of the non-null variant type
                return print_struct_val_starting_from(0, val.GetChildAtIndex(0), internal_dict)
        else:
            # This is just a regular uni-variant enum without discriminator field
            return print_struct_val_starting_from(0, val.GetChildAtIndex(0), internal_dict)

    # If we are here, this is a regular enum with more than one variant
    disr_val = val.GetChildAtIndex(0).GetChildMemberWithName("RUST$ENUM$DISR")
    disr_type = disr_val.GetType()

    if disr_type.GetTypeClass() != lldb.eTypeClassEnumeration:
        return "<Invalid enum value encountered: Discriminator is not an enum>"

    variant_index = disr_val.GetValueAsUnsigned()
    return print_struct_val_starting_from(1, val.GetChildAtIndex(variant_index), internal_dict)


def print_pointer_val(val, internal_dict):
    '''Prints a pointer value with Rust syntax'''
    assert val.GetType().IsPointerType()
    sigil = "&"
    type_name = extract_type_name(val.GetType().GetName())
    if type_name and type_name[0:1] in ["&", "~", "*"]:
        sigil = type_name[0:1]

    return sigil + hex(val.GetValueAsUnsigned()) #print_val(val.Dereference(), internal_dict)


def print_fixed_size_vec_val(val, internal_dict):
    assert val.GetType().GetTypeClass() == lldb.eTypeClassArray

    output = "["

    for i in range(val.num_children):
        output += print_val(val.GetChildAtIndex(i), internal_dict)
        if i != val.num_children - 1:
            output += ", "

    output += "]"
    return output


def print_vec_slice_val(val, internal_dict):
    length = val.GetChildAtIndex(1).GetValueAsUnsigned()

    data_ptr_val = val.GetChildAtIndex(0)
    data_ptr_type = data_ptr_val.GetType()

    return "&[%s]" % print_array_of_values(val.GetName(),
                                           data_ptr_val,
                                           length,
                                           internal_dict)


def print_std_vec_val(val, internal_dict):
    length = val.GetChildAtIndex(1).GetValueAsUnsigned()

    # Vec<> -> Unique<> -> NonZero<> -> *T
    data_ptr_val = val.GetChildAtIndex(0).GetChildAtIndex(0).GetChildAtIndex(0)
    data_ptr_type = data_ptr_val.GetType()

    return "vec![%s]" % print_array_of_values(val.GetName(),
                                              data_ptr_val,
                                              length,
                                              internal_dict)

#=--------------------------------------------------------------------------------------------------
# Helper Functions
#=--------------------------------------------------------------------------------------------------

unqualified_type_markers = frozenset(["(", "[", "&", "*"])


def extract_type_name(qualified_type_name):
    '''Extracts the type name from a fully qualified path'''
    if qualified_type_name[0] in unqualified_type_markers:
        return qualified_type_name

    end_of_search = qualified_type_name.find("<")
    if end_of_search < 0:
        end_of_search = len(qualified_type_name)

    index = qualified_type_name.rfind("::", 0, end_of_search)
    if index < 0:
        return qualified_type_name
    else:
        return qualified_type_name[index + 2:]


def type_has_field_names(ty):
    '''Returns true of this is a type with field names (struct, struct-like enum variant)'''
    # This may also be an enum variant where the first field doesn't have a name but the rest has
    if ty.GetNumberOfFields() > 1:
        return ty.GetFieldAtIndex(1).GetName() is not None
    else:
        return ty.GetFieldAtIndex(0).GetName() is not None


def is_vec_slice(val):
    ty = val.GetType()
    if ty.GetTypeClass() != lldb.eTypeClassStruct:
        return False

    if ty.GetNumberOfFields() != 2:
        return False

    if ty.GetFieldAtIndex(0).GetName() != "data_ptr":
        return False

    if ty.GetFieldAtIndex(1).GetName() != "length":
        return False

    type_name = extract_type_name(ty.GetName()).replace("&'static", "&").replace(" ", "")
    return type_name.startswith("&[") and type_name.endswith("]")

def is_std_vec(val):
    ty = val.GetType()
    if ty.GetTypeClass() != lldb.eTypeClassStruct:
        return False

    if ty.GetNumberOfFields() != 3:
        return False

    if ty.GetFieldAtIndex(0).GetName() != "ptr":
        return False

    if ty.GetFieldAtIndex(1).GetName() != "len":
        return False

    if ty.GetFieldAtIndex(2).GetName() != "cap":
        return False

    return ty.GetName().startswith("collections::vec::Vec<")


def print_array_of_values(array_name, data_ptr_val, length, internal_dict):
    '''Prints a contigous memory range, interpreting it as values of the
       pointee-type of data_ptr_val.'''

    data_ptr_type = data_ptr_val.GetType()
    assert data_ptr_type.IsPointerType()

    element_type = data_ptr_type.GetPointeeType()
    element_type_size = element_type.GetByteSize()

    start_address = data_ptr_val.GetValueAsUnsigned()

    def render_element(i):
        address = start_address + i * element_type_size
        element_val = data_ptr_val.CreateValueFromAddress(array_name + ("[%s]" % i),
                                                          address,
                                                          element_type)
        return print_val(element_val, internal_dict)

    return ', '.join([render_element(i) for i in range(length)])
