/**
 *    Copyright (C) 2020-present MongoDB, Inc.
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

#include <variant>

#include <boost/smart_ptr/intrusive_ptr.hpp>

#include "mongo/base/string_data.h"
#include "mongo/bson/bsonelement.h"
#include "mongo/bson/bsonobj.h"
#include "mongo/bson/json.h"
#include "mongo/db/cst/bson_lexer.h"
#include "mongo/db/cst/c_node.h"
#include "mongo/db/cst/cst_match_translation.h"
#include "mongo/db/cst/parser_gen.hpp"
#include "mongo/db/matcher/expression_leaf.h"
#include "mongo/db/matcher/expression_tree.h"
#include "mongo/db/matcher/expression_type.h"
#include "mongo/db/matcher/extensions_callback_noop.h"
#include "mongo/db/matcher/matcher_type_set.h"
#include "mongo/db/namespace_string.h"
#include "mongo/db/pipeline/expression_context_for_test.h"
#include "mongo/unittest/assert.h"
#include "mongo/unittest/bson_test_util.h"
#include "mongo/unittest/framework.h"
#include "mongo/util/intrusive_counter.h"

namespace mongo {
namespace {

auto getExpCtx() {
    auto nss = NamespaceString::createNamespaceString_forTest("db", "coll");
    return boost::intrusive_ptr<ExpressionContextForTest>{new ExpressionContextForTest(nss)};
}

auto translate(const CNode& cst) {
    return cst_match_translation::translateMatchExpression(
        cst, getExpCtx(), ExtensionsCallbackNoop{});
}

auto parseMatchToCst(BSONObj input) {
    CNode output;
    BSONLexer lexer(input["filter"].embeddedObject(), ParserGen::token::START_MATCH);
    auto parseTree = ParserGen(lexer, &output);
    ASSERT_EQ(0, parseTree.parse());
    return output;
}

TEST(CstMatchTranslationTest, TranslatesEmpty) {
    const auto cst = CNode{CNode::ObjectChildren{}};
    auto match = translate(cst);
    auto andExpr = dynamic_cast<AndMatchExpression*>(match.get());
    ASSERT(andExpr);
    ASSERT_EQ(0, andExpr->numChildren());
}

TEST(CstMatchTranslationTest, TranslatesSinglePredicate) {
    const auto cst = CNode{CNode::ObjectChildren{{UserFieldname{"a"}, CNode{UserInt{1}}}}};
    auto match = translate(cst);
    ASSERT_BSONOBJ_EQ(match->serialize(), fromjson("{$and: [{a: {$eq: 1}}]}"));
}

TEST(CstMatchTranslationTest, TranslatesMultipleEqualityPredicates) {
    const auto cst = CNode{CNode::ObjectChildren{
        {UserFieldname{"a"}, CNode{UserInt{1}}},
        {UserFieldname{"b"}, CNode{UserNull{}}},
    }};
    auto match = translate(cst);
    ASSERT_BSONOBJ_EQ(match->serialize(), fromjson("{$and: [{a: {$eq: 1}}, {b: {$eq: null}}]}"));
}

TEST(CstMatchTranslationTest, TranslatesEqualityPredicatesWithId) {
    const auto cst = CNode{CNode::ObjectChildren{
        {UserFieldname{"_id"}, CNode{UserNull{}}},
    }};
    auto match = translate(cst);
    auto andExpr = dynamic_cast<AndMatchExpression*>(match.get());
    ASSERT(andExpr);
    ASSERT_EQ(1, andExpr->numChildren());
    ASSERT_BSONOBJ_EQ(match->serialize(), fromjson("{$and: [{_id: {$eq: null}}]}"));
}

TEST(CstMatchTranslationTest, TranslatesEmptyObject) {
    const auto cst = CNode{CNode::ObjectChildren{}};
    auto match = translate(cst);
    auto andExpr = dynamic_cast<AndMatchExpression*>(match.get());
    ASSERT(andExpr);
    ASSERT_EQ(0, andExpr->numChildren());
}

TEST(CstMatchTranslationTest, TranslatesNotWithRegex) {
    auto input = fromjson("{filter: {a: {$not: /b/}}}");
    auto cst = parseMatchToCst(input);
    auto match = translate(cst);
    auto andExpr = dynamic_cast<AndMatchExpression*>(match.get());
    ASSERT(andExpr);
    ASSERT_EQ(1, andExpr->numChildren());
    auto notExpr = dynamic_cast<NotMatchExpression*>(andExpr->getChild(0));
    ASSERT(notExpr);
    auto regex = dynamic_cast<RegexMatchExpression*>(notExpr->getChild(0));
    ASSERT(regex);
    ASSERT_EQ("a", regex->path());
    ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $not: { $regex: \"b\" } } } ] }");
}

TEST(CstMatchTranslationTest, TranslatesNotWithExpression) {
    auto input = fromjson("{filter: {a: {$not: {$not: /b/}}}}");
    auto cst = parseMatchToCst(input);
    auto match = translate(cst);
    ASSERT_EQ(match->serialize().toString(),
              "{ $and: [ { $nor: [ { a: { $not: { $regex: \"b\" } } } ] } ] }");
}

TEST(CstMatchTranslationTest, TranslatesLogicalTreeExpressions) {
    {
        auto input = fromjson("{filter: {$and: [{b: {$not: /a/}}]}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { $and: [ { $and: [ { b: { $not: { $regex: \"a\" } } } ] } ] } ] }");
    }
    {
        auto input = fromjson("{filter: {$or: [{b: 1}, {a: 2}]}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { $or: [ { $and: [ { b: { $eq: 1 } } ] }, { $and: [ { a: { $eq: 2 } } "
                  "] } ] } ] }");
    }
    {
        auto input = fromjson("{filter: {$nor: [{b: {$not: /a/}}]}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { $nor: [ { $and: [ { b: { $not: { $regex: \"a\" } } } ] } ] } ] }");
    }
}

TEST(CstMatchTranslationTest, TranslatesNestedLogicalTreeExpressions) {
    {
        auto input = fromjson("{filter: {$and: [{$or: [{b: {$not: /a/}}]}]}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { $and: [ { $and: [ { $or: [ { $and: [ { b: { $not: { $regex: \"a\" } "
                  "} } ] } ] } ] } ] } ] }");
    }
    {
        auto input = fromjson("{filter: {$or: [{$and: [{b: {$not: /a/}}, {a: {$not: /b/}}]}]}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { $or: [ { $and: [ { $and: [ { $and: [ { b: { $not: { $regex: \"a\" } "
                  "} } ] }, { $and: [ { a: { $not: { $regex: \"b\" } } } ] } ] } ] } ] } ] }");
    }
    {
        auto input = fromjson("{filter: {$and: [{$nor: [{b: {$not: /a/}}]}]}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { $and: [ { $and: [ { $nor: [ { $and: [ { b: { $not: { $regex: \"a\" "
                  "} } } ] } ] } ] } ] } ] }");
    }
}

TEST(CstMatchTranslationTest, TranslatesExistsBool) {
    {
        auto input = fromjson("{filter: {a: {$exists: true}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $exists: true } } ] }");
    }
    {
        auto input = fromjson("{filter: {a: {$exists: false}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { a: { $not: { $exists: true } } } ] }");
    }
}

TEST(CstMatchTranslationTest, TranslatesExistsNumeric) {
    {
        auto input = fromjson("{filter: {a: {$exists: 15.0}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $exists: true } } ] }");
    }
    {
        auto input = fromjson("{filter: {a: {$exists: 0}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { a: { $not: { $exists: true } } } ] }");
    }
}

TEST(CstMatchTranslationTest, TranslatesExistsNullAndCompound) {
    {
        auto input = fromjson("{filter: {a: {$exists: null}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { a: { $not: { $exists: true } } } ] }");
    }
    {
        auto input = fromjson("{filter: {a: {$exists: [\"arbitrary stuff\", null]}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $exists: true } } ] }");
    }
    {
        auto input = fromjson("{filter: {a: {$exists: {doesnt: \"matter\"}}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $exists: true } } ] }");
    }
}

TEST(CstMatchTranslationTest, TranslatesType) {
    {
        auto input = fromjson("{filter: {a: {$type: 1}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $type: [ 1 ] } } ] }");
    }
    {
        auto input = fromjson("{filter: {a: {$type: \"number\"}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $type: [ \"number\" ] } } ] }");
        // The compound "number" alias is not translated; instead the allNumbers flag of the typeset
        // used by the MatchExpression is set.
        auto andExpr = dynamic_cast<AndMatchExpression*>(match.get());
        ASSERT(andExpr);
        ASSERT_EQ(1, andExpr->numChildren());
        auto type_match = dynamic_cast<TypeMatchExpression*>(andExpr->getChild(0));
        ASSERT(type_match);
        ASSERT(type_match->typeSet().allNumbers);
    }
    {
        auto input = fromjson("{filter: {a: {$type: [ \"number\", \"string\", 11]}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(),
                  "{ $and: [ { a: { $type: [ \"number\", 2, 11 ] } } ] }");
        // Direct type aliases (like "string" --> BSONType 2) are translated into their numeric
        // type.
        auto andExpr = dynamic_cast<AndMatchExpression*>(match.get());
        ASSERT(andExpr);
        ASSERT_EQ(1, andExpr->numChildren());
        auto type_match = dynamic_cast<TypeMatchExpression*>(andExpr->getChild(0));
        ASSERT(type_match->typeSet().allNumbers);
    }
}

TEST(CstMatchTranslationTest, TranslatesComment) {
    {
        auto input = fromjson("{filter: {a: 1, $comment: \"hello, world\"}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $eq: 1 } } ] }");
    }
    {
        auto input = fromjson("{filter: {$comment: \"hello, world\"}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        auto andExpr = dynamic_cast<AndMatchExpression*>(match.get());
        ASSERT(andExpr);
        ASSERT_EQ(0, andExpr->numChildren());
    }
    {
        auto input = fromjson("{filter: {a: {$exists: true}, $comment: \"hello, world\"}}}");
        auto cst = parseMatchToCst(input);
        auto match = translate(cst);
        ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $exists: true } } ] }");
    }
}

TEST(CstMatchTranslationTest, TranslatesExpr) {
    auto input = fromjson("{filter: {$expr: 123}}");
    auto cst = parseMatchToCst(input);
    auto match = translate(cst);
    ASSERT_EQ(match->serialize().toString(), "{ $and: [ { $expr: { $const: 123 } } ] }");
}

TEST(CstMatchTranslationTest, TranslatesText) {
    auto input = fromjson("{filter: {$text: {$search: \"hi\"}}}");
    auto cst = parseMatchToCst(input);
    auto match = translate(cst);
    ASSERT_EQ(match->serialize().toString(),
              "{ $and: [ "
              "{ $text: { $search: \"hi\", $language: \"\", "
              "$caseSensitive: false, $diacriticSensitive: false } } ] }");
}

TEST(CstMatchTranslationTest, TranslatesWhere) {
    auto input = fromjson("{filter: {$where: \"return this.q\"}}");
    auto cst = parseMatchToCst(input);
    auto match = translate(cst);
    ASSERT_EQ(match->serialize().toString(),
              "{ $and: [ "
              "{ $where: return this.q } ] }");
}

TEST(CstMatchTranslationTest, TranslatesMod) {
    auto input = fromjson("{filter: {a: {$mod: [3, 2.0]}}}");
    auto cst = parseMatchToCst(input);
    auto match = translate(cst);
    ASSERT_EQ(match->serialize().toString(), "{ $and: [ { a: { $mod: [ 3, 2 ] } } ] }");
}

}  // namespace
}  // namespace mongo
