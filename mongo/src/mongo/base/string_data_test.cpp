/**
 *    Copyright (C) 2018-present MongoDB, Inc.
 *
 *    This program is free software: you can redistribute it and/or modify
 *    it under the terms of the Server Side Public License, version 1,
 *    as published by MongoDB, Inc.
 *
 *    This program is distributed in the hope that it will be useful,
 *    but WITHOUT ANY WARRANTY; without even the implied warranty of
 *    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *    Server Side Public License for more details.
 *
 *    You should have received a copy of the Server Side Public License
 *    along with this program. If not, see
 *    <http://www.mongodb.com/licensing/server-side-public-license>.
 *
 *    As a special exception, the copyright holders give permission to link the
 *    code of portions of this program with the OpenSSL library under certain
 *    conditions as described in each individual source file and distribute
 *    linked combinations including the program with the OpenSSL library. You
 *    must comply with the Server Side Public License in all respects for
 *    all of the code used other than as permitted herein. If you modify file(s)
 *    with this exception, you may extend this exception to your version of the
 *    file(s), but you are not obligated to do so. If you do not wish to do so,
 *    delete this exception statement from your version. If you delete this
 *    exception statement from all source files in the program, then also delete
 *    it in the license file.
 */

#include <algorithm>
#include <functional>
#include <initializer_list>
#include <iomanip>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

#include <fmt/format.h>

#include "mongo/base/simple_string_data_comparator.h"
#include "mongo/base/string_data.h"
#include "mongo/config.h"  // IWYU pragma: keep
#include "mongo/unittest/assert.h"
#include "mongo/unittest/death_test.h"
#include "mongo/unittest/framework.h"

namespace mongo {
namespace {

using std::string;

TEST(Construction, Empty) {
    StringData strData;
    ASSERT_EQUALS(strData.size(), 0U);
    ASSERT_TRUE(strData.rawData() == nullptr);
}

TEST(Construction, FromStdString) {
    std::string base("aaa");
    StringData strData(base);
    ASSERT_EQUALS(strData.size(), base.size());
    ASSERT_EQUALS(strData.toString(), base);
}

TEST(Construction, FromCString) {
    std::string base("aaa");
    StringData strData(base.c_str());
    ASSERT_EQUALS(strData.size(), base.size());
    ASSERT_EQUALS(strData.toString(), base);
}

TEST(Construction, FromNullCString) {
    char* c = nullptr;
    StringData strData(c);
    ASSERT_EQUALS(strData.size(), 0U);
    ASSERT_TRUE(strData.rawData() == nullptr);
}

TEST(Construction, FromUserDefinedLiteral) {
    const auto strData = "cc\0c"_sd;
    ASSERT_EQUALS(strData.size(), 4U);
    ASSERT_EQUALS(strData.toString(), string("cc\0c", 4));
}

TEST(Construction, FromUserDefinedRawLiteral) {
    const auto strData = R"("")"_sd;
    ASSERT_EQUALS(strData.size(), 2U);
    ASSERT_EQUALS(strData.toString(), string("\"\"", 2));
}

TEST(Construction, FromEmptyUserDefinedLiteral) {
    const auto strData = ""_sd;
    ASSERT_EQUALS(strData.size(), 0U);
    ASSERT_EQUALS(strData.toString(), string(""));
}

// Try some constexpr initializations
TEST(Construction, Constexpr) {
    constexpr StringData lit = "1234567"_sd;
    ASSERT_EQUALS(lit, "1234567"_sd);
    constexpr StringData sub = lit.substr(3, 2);
    ASSERT_EQUALS(sub, "45"_sd);
    constexpr StringData range(lit.begin() + 1, lit.end() - 1);
    ASSERT_EQUALS(range, "23456"_sd);
    constexpr char c = lit[1];
    ASSERT_EQUALS(c, '2');
    constexpr StringData nully{nullptr, 0};
    ASSERT_EQUALS(nully, ""_sd);
#if 0
    constexpr StringData cxNully{nullptr, 1};  // must not compile
#endif
    constexpr StringData ptr{lit.rawData() + 1, 3};
    ASSERT_EQUALS(ptr, "234"_sd);
}

class StringDataDeathTest : public unittest::Test {};

#if defined(MONGO_CONFIG_DEBUG_BUILD)
DEATH_TEST(StringDataDeathTest,
           InvariantNullRequiresEmpty,
           "StringData(nullptr,len) requires len==0") {
    [[maybe_unused]] StringData bad{nullptr, 1};
}
#endif

TEST(Comparison, BothEmpty) {
    StringData empty("");
    ASSERT_TRUE(empty == empty);
    ASSERT_FALSE(empty != empty);
    ASSERT_FALSE(empty > empty);
    ASSERT_TRUE(empty >= empty);
    ASSERT_FALSE(empty < empty);
    ASSERT_TRUE(empty <= empty);

    static_assert(""_sd.compare(""_sd) == 0);
}

TEST(Comparison, BothNonEmptyOnSize) {
    StringData a("a");
    StringData aa("aa");
    ASSERT_FALSE(a == aa);
    ASSERT_TRUE(a != aa);
    ASSERT_FALSE(a > aa);
    ASSERT_FALSE(a >= aa);
    ASSERT_TRUE(a >= a);
    ASSERT_TRUE(a < aa);
    ASSERT_TRUE(a <= aa);
    ASSERT_TRUE(a <= a);

    static_assert("a"_sd.compare("aa"_sd) < 0);
}

TEST(Comparison, BothNonEmptyOnContent) {
    StringData a("a");
    StringData b("b");
    ASSERT_FALSE(a == b);
    ASSERT_TRUE(a != b);
    ASSERT_FALSE(a > b);
    ASSERT_FALSE(a >= b);
    ASSERT_TRUE(a < b);
    ASSERT_TRUE(a <= b);

    static_assert("a"_sd.compare("b"_sd) < 0);
}

TEST(Comparison, MixedEmptyAndNot) {
    StringData empty("");
    StringData a("a");
    ASSERT_FALSE(a == empty);
    ASSERT_TRUE(a != empty);
    ASSERT_TRUE(a > empty);
    ASSERT_TRUE(a >= empty);
    ASSERT_FALSE(a < empty);
    ASSERT_FALSE(a <= empty);

    static_assert(""_sd.compare("a"_sd) < 0);
}

TEST(Find, Char1) {
    ASSERT_EQUALS(string::npos, StringData("foo").find('a'));
    ASSERT_EQUALS(0U, StringData("foo").find('f'));
    ASSERT_EQUALS(1U, StringData("foo").find('o'));

    using namespace std::literals;
    const std::string haystacks[]{"foo", "f", "", "\0"s, "f\0"s, "\0f"s, "ffoo", "afoo"};
    const char needles[]{'a', 'f', 'o', '\0'};
    for (const auto& s : haystacks) {
        for (const auto& ch : needles) {
            // Try all possibly-relevent `pos` arguments.
            for (size_t pos = 0; pos < s.size() + 2; ++pos) {
                // All expectations should be consistent with std::string::find.
                auto withStdString = s.find(ch, pos);
                auto withStringData = StringData{s}.find(ch, pos);
                ASSERT_EQUALS(withStdString, withStringData)
                    << format(FMT_STRING(R"(s:'{}', ch:'{}', pos:{})"), s, StringData{&ch, 1}, pos);
            }
        }
    }
}

TEST(Find, Str1) {
    ASSERT_EQUALS(string::npos, StringData("foo").find("asdsadasda"));
    ASSERT_EQUALS(string::npos, StringData("foo").find("a"));
    ASSERT_EQUALS(string::npos, StringData("foo").find("food"));
    ASSERT_EQUALS(string::npos, StringData("foo").find("ooo"));

    ASSERT_EQUALS(0U, StringData("foo").find("f"));
    ASSERT_EQUALS(0U, StringData("foo").find("fo"));
    ASSERT_EQUALS(0U, StringData("foo").find("foo"));
    ASSERT_EQUALS(1U, StringData("foo").find("o"));
    ASSERT_EQUALS(1U, StringData("foo").find("oo"));

    ASSERT_EQUALS(string("foo").find(""), StringData("foo").find(""));

    using namespace std::literals;
    const std::string haystacks[]{"", "x", "foo", "fffoo", "\0"s};
    const std::string needles[]{
        "", "x", "asdsadasda", "a", "f", "fo", "foo", "food", "o", "oo", "ooo", "\0"s};
    for (const auto& s : haystacks) {
        for (const auto& sub : needles) {
            // Try all possibly-relevent `pos` arguments.
            for (size_t pos = 0; pos < std::max(s.size(), sub.size()) + 2; ++pos) {
                // All expectations should be consistent with std::string::find.
                auto withStdString = s.find(sub, pos);
                auto withStringData = StringData{s}.find(StringData{sub}, pos);
                ASSERT_EQUALS(withStdString, withStringData)
                    << format(FMT_STRING(R"(s:'{}', sub:'{}', pos:{})"), s, sub, pos);
            }
        }
    }
}

// Helper function for Test(Hasher, Str1)
template <int SizeofSizeT>
void SDHasher_check(void);

template <>
void SDHasher_check<4>(void) {
    const auto& strCmp = SimpleStringDataComparator::kInstance;
    ASSERT_EQUALS(strCmp.hash(""), static_cast<size_t>(0));
    ASSERT_EQUALS(strCmp.hash("foo"), static_cast<size_t>(4138058784ULL));
    ASSERT_EQUALS(strCmp.hash("pizza"), static_cast<size_t>(3587803311ULL));
    ASSERT_EQUALS(strCmp.hash("mongo"), static_cast<size_t>(3724335885ULL));
    ASSERT_EQUALS(strCmp.hash("murmur"), static_cast<size_t>(1945310157ULL));
}

template <>
void SDHasher_check<8>(void) {
    const auto& strCmp = SimpleStringDataComparator::kInstance;
    ASSERT_EQUALS(strCmp.hash(""), static_cast<size_t>(0));
    ASSERT_EQUALS(strCmp.hash("foo"), static_cast<size_t>(16316970633193145697ULL));
    ASSERT_EQUALS(strCmp.hash("pizza"), static_cast<size_t>(12165495155477134356ULL));
    ASSERT_EQUALS(strCmp.hash("mongo"), static_cast<size_t>(2861051452199491487ULL));
    ASSERT_EQUALS(strCmp.hash("murmur"), static_cast<size_t>(18237957392784716687ULL));
}

TEST(Hasher, Str1) {
    SDHasher_check<sizeof(size_t)>();
}

TEST(Rfind, Char1) {
    ASSERT_EQUALS(string::npos, StringData("foo").rfind('a'));

    ASSERT_EQUALS(0U, StringData("foo").rfind('f'));
    ASSERT_EQUALS(0U, StringData("foo").rfind('f', 3));
    ASSERT_EQUALS(0U, StringData("foo").rfind('f', 2));
    ASSERT_EQUALS(0U, StringData("foo").rfind('f', 1));
    ASSERT_EQUALS(string::npos, StringData("foo", 0).rfind('f'));

    ASSERT_EQUALS(2U, StringData("foo").rfind('o'));
    ASSERT_EQUALS(2U, StringData("foo", 3).rfind('o'));
    ASSERT_EQUALS(1U, StringData("foo", 2).rfind('o'));
    ASSERT_EQUALS(string::npos, StringData("foo", 1).rfind('o'));
    ASSERT_EQUALS(string::npos, StringData("foo", 0).rfind('o'));

    using namespace std::literals;
    const std::string haystacks[]{"", "x", "foo", "fffoo", "oof", "\0"s};
    const char needles[]{'f', 'o', '\0'};
    for (const auto& s : haystacks) {
        for (const auto& ch : needles) {
            auto validate = [&](size_t pos) {
                // All expectations should be consistent with std::string::rfind.
                auto withStdString = s.rfind(ch, pos);
                auto withStringData = StringData{s}.rfind(ch, pos);
                ASSERT_EQUALS(withStdString, withStringData)
                    << format(FMT_STRING(R"(s:'{}', ch:'{}', pos:{})"), s, StringData{&ch, 1}, pos);
            };
            // Try all possibly-relevent `pos` arguments.
            for (size_t pos = 0; pos < s.size() + 2; ++pos)
                validate(pos);
            validate(std::string::npos);
        }
    }
}

// this is to verify we match std::string
void SUBSTR_TEST_HELP(StringData big, StringData small, size_t start, size_t len) {
    ASSERT_EQUALS(small.toString(), big.toString().substr(start, len));
    ASSERT_EQUALS(small, StringData(big).substr(start, len));
}
void SUBSTR_TEST_HELP(StringData big, StringData small, size_t start) {
    ASSERT_EQUALS(small.toString(), big.toString().substr(start));
    ASSERT_EQUALS(small, StringData(big).substr(start));
}

// [12] is number of args to substr
#define SUBSTR_1_TEST_HELP(big, small, start)                                              \
    ASSERT_EQUALS(StringData(small).toString(), StringData(big).toString().substr(start)); \
    ASSERT_EQUALS(StringData(small), StringData(big).substr(start));

#define SUBSTR_2_TEST_HELP(big, small, start, len)                                              \
    ASSERT_EQUALS(StringData(small).toString(), StringData(big).toString().substr(start, len)); \
    ASSERT_EQUALS(StringData(small), StringData(big).substr(start, len));

TEST(Substr, Simple1) {
    SUBSTR_1_TEST_HELP("abcde", "abcde", 0);
    SUBSTR_2_TEST_HELP("abcde", "abcde", 0, 10);
    SUBSTR_2_TEST_HELP("abcde", "abcde", 0, 5);
    SUBSTR_2_TEST_HELP("abcde", "abc", 0, 3);
    SUBSTR_1_TEST_HELP("abcde", "cde", 2);
    SUBSTR_2_TEST_HELP("abcde", "cde", 2, 5);
    SUBSTR_2_TEST_HELP("abcde", "cde", 2, 3);
    SUBSTR_2_TEST_HELP("abcde", "cd", 2, 2);
    SUBSTR_2_TEST_HELP("abcde", "cd", 2, 2);
    SUBSTR_1_TEST_HELP("abcde", "", 5);
    SUBSTR_2_TEST_HELP("abcde", "", 5, 0);
    SUBSTR_2_TEST_HELP("abcde", "", 5, 10);

    // make sure we don't blow past the end of the StringData
    SUBSTR_1_TEST_HELP(StringData("abcdeXXX", 5), "abcde", 0);
    SUBSTR_2_TEST_HELP(StringData("abcdeXXX", 5), "abcde", 0, 10);
    SUBSTR_1_TEST_HELP(StringData("abcdeXXX", 5), "de", 3);
    SUBSTR_2_TEST_HELP(StringData("abcdeXXX", 5), "de", 3, 7);
    SUBSTR_1_TEST_HELP(StringData("abcdeXXX", 5), "", 5);
    SUBSTR_2_TEST_HELP(StringData("abcdeXXX", 5), "", 5, 1);
}

TEST(equalCaseInsensitiveTest, Simple1) {
    ASSERT(StringData("abc").equalCaseInsensitive("abc"));
    ASSERT(StringData("abc").equalCaseInsensitive("ABC"));
    ASSERT(StringData("ABC").equalCaseInsensitive("abc"));
    ASSERT(StringData("ABC").equalCaseInsensitive("ABC"));
    ASSERT(StringData("ABC").equalCaseInsensitive("AbC"));
    ASSERT(!StringData("ABC").equalCaseInsensitive("AbCd"));
    ASSERT(!StringData("ABC").equalCaseInsensitive("AdC"));
}

TEST(StartsWith, Simple) {
    ASSERT(StringData("").startsWith(""));
    ASSERT(!StringData("").startsWith("x"));
    ASSERT(StringData("abcde").startsWith(""));
    ASSERT(StringData("abcde").startsWith("a"));
    ASSERT(StringData("abcde").startsWith("ab"));
    ASSERT(StringData("abcde").startsWith("abc"));
    ASSERT(StringData("abcde").startsWith("abcd"));
    ASSERT(StringData("abcde").startsWith("abcde"));
    ASSERT(!StringData("abcde").startsWith("abcdef"));
    ASSERT(!StringData("abcde").startsWith("abdce"));
    ASSERT(StringData("abcde").startsWith(StringData("abcdeXXXX").substr(0, 4)));
    ASSERT(!StringData("abcde").startsWith(StringData("abdef").substr(0, 4)));
    ASSERT(!StringData("abcde").substr(0, 3).startsWith("abcd"));
}

TEST(EndsWith, Simple) {
    // ASSERT(StringData("").endsWith(""));
    ASSERT(!StringData("").endsWith("x"));
    // ASSERT(StringData("abcde").endsWith(""));
    ASSERT(StringData("abcde").endsWith(StringData("e", 0)));
    ASSERT(StringData("abcde").endsWith("e"));
    ASSERT(StringData("abcde").endsWith("de"));
    ASSERT(StringData("abcde").endsWith("cde"));
    ASSERT(StringData("abcde").endsWith("bcde"));
    ASSERT(StringData("abcde").endsWith("abcde"));
    ASSERT(!StringData("abcde").endsWith("0abcde"));
    ASSERT(!StringData("abcde").endsWith("abdce"));
    ASSERT(StringData("abcde").endsWith(StringData("bcdef").substr(0, 4)));
    ASSERT(!StringData("abcde").endsWith(StringData("bcde", 3)));
    ASSERT(!StringData("abcde").substr(0, 3).endsWith("cde"));
}

TEST(ConstIterator, StdCopy) {
    std::vector<char> chars;
    auto data = "This is some raw data."_sd;

    chars.resize(data.size());
    std::copy(data.begin(), data.end(), chars.begin());

    for (size_t i = 0; i < data.size(); ++i) {
        ASSERT_EQUALS(data[i], chars[i]);
    }
}

TEST(ConstIterator, StdReverseCopy) {
    std::vector<char> chars;
    auto data = "This is some raw data."_sd;

    chars.resize(data.size());
    std::reverse_copy(data.begin(), data.end(), chars.begin());

    const char rawDataExpected[] = ".atad war emos si sihT";

    for (size_t i = 0; i < data.size(); ++i) {
        ASSERT_EQUALS(rawDataExpected[i], chars[i]);
    }
}

TEST(ConstIterator, StdReplaceCopy) {
    std::vector<char> chars;
    auto data = "This is some raw data."_sd;

    chars.resize(data.size());
    std::replace_copy(data.begin(), data.end(), chars.begin(), ' ', '_');

    const char rawDataExpected[] = "This_is_some_raw_data.";

    for (size_t i = 0; i < data.size(); ++i) {
        ASSERT_EQUALS(rawDataExpected[i], chars[i]);
    }
}

TEST(StringDataFmt, Fmt) {
    using namespace fmt::literals;
    ASSERT_EQUALS(fmt::format("-{}-", "abc"_sd), "-abc-");
    ASSERT_EQUALS("-{}-"_format("abc"_sd), "-abc-");
}

TEST(Ostream, StringDataMatchesStdString) {
    const std::string s = "xyz";
    struct TestCase {
        int line;
        std::function<void(std::ostream&)> manip;
    };
    const TestCase testCases[] = {
        {__LINE__,
         [](std::ostream& os) {
         }},
        {__LINE__,
         [](std::ostream& os) {
             os << std::setw(5);
         }},
        {__LINE__,
         [](std::ostream& os) {
             os << std::left << std::setw(5);
         }},
        {__LINE__,
         [](std::ostream& os) {
             os << std::right << std::setw(5);
         }},
        {__LINE__,
         [](std::ostream& os) {
             os << std::setfill('.') << std::left << std::setw(5);
         }},
        {__LINE__,
         [](std::ostream& os) {
             os << std::setfill('.') << std::right << std::setw(5);
         }},
    };
    for (const auto& testCase : testCases) {
        const std::string location = std::string(" at line:") + std::to_string(testCase.line);
        struct Experiment {
            Experiment(std::function<void(std::ostream&)> f) : putter(f) {}
            std::function<void(std::ostream&)> putter;
            std::ostringstream os;
        };
        Experiment expected{[&](std::ostream& os) {
            os << s;
        }};
        Experiment actual{[&](std::ostream& os) {
            os << StringData(s);
        }};
        for (auto& x : {&expected, &actual}) {
            x->os << ">>";
            testCase.manip(x->os);
            x->putter(x->os);
        }
        // ASSERT_EQ(expected.os.str(), actual.os.str()) << location;
        for (auto& x : {&expected, &actual}) {
            x->os << "<<";
        }
        ASSERT_EQ(expected.os.str(), actual.os.str()) << location;
    }
}

TEST(StringData, PlusEq) {
    auto str = std::string("hello ");
    auto& ret = str += "world"_sd;
    ASSERT_EQ(str, "hello world");
    ASSERT_EQ(&ret, &str);
}

}  // namespace
}  // namespace mongo
