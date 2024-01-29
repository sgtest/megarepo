"""Script to be invoked by GDB for testing decorable pretty printing.
"""

import gdb
import re

expected_patterns = [
    r'Decorable<MyDecorable\> with 3 elems',
    r'vector of length 3.*\{ *123, *213, *312 *\}',
    r'basic_string.* \= *"hello"',
    r'basic_string.* \= *"world"',
]
up_pattern = r'std::unique_ptr<int\> = \{get\(\) \= 0x[0-9a-fA-F]+\}'
set_pattern = r'std::[__debug::]*set with 4 elements'
static_member_pattern = '128'


def search(pattern, s):
    match = re.search(pattern, s)
    assert match is not None, 'Did not find {!s} in {!s}'.format(pattern, s)
    return match


def test_decorable():
    gdb.execute('run')
    gdb.execute('frame function main')
    d1_str = gdb.execute('print d1', to_string=True)
    for pattern in expected_patterns:
        search(pattern, d1_str)

    search(up_pattern, gdb.execute('print up', to_string=True))
    search(set_pattern, gdb.execute('print set_type', to_string=True))
    search(static_member_pattern, gdb.execute('print testClass::static_member', to_string=True))


def test_dbname_nss():
    gdb.execute('run')
    gdb.execute('frame function main')
    dbname_str = gdb.execute('print dbName', to_string=True)
    search("foo", dbname_str)
    dbname_tid_str = gdb.execute('print dbNameWithTenantId', to_string=True)
    search("6491a2112ef5c818703bf2a7_foo", dbname_tid_str)
    nss_str = gdb.execute('print nss', to_string=True)
    search("foo.bar", nss_str)
    nss_tid_str = gdb.execute('print nssWithTenantId', to_string=True)
    search("6491a2112ef5c818703bf2a7_foo.bar", nss_tid_str)


if __name__ == '__main__':
    try:
        test_decorable()
        test_dbname_nss()
        gdb.write('TEST PASSED\n')
    except Exception as err:
        gdb.write('TEST FAILED -- {!s}\n'.format(err))
        gdb.execute('quit 1', to_string=True)
