/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
// ANTLR GENERATED CODE: DO NOT EDIT
package org.elasticsearch.xpack.sql.parser;
import org.antlr.v4.runtime.tree.ParseTreeVisitor;

/**
 * This interface defines a complete generic visitor for a parse tree produced
 * by {@link SqlBaseParser}.
 *
 * @param <T> The return type of the visit operation. Use {@link Void} for
 * operations with no return type.
 */
interface SqlBaseVisitor<T> extends ParseTreeVisitor<T> {
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#singleStatement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSingleStatement(SqlBaseParser.SingleStatementContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#singleExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSingleExpression(SqlBaseParser.SingleExpressionContext ctx);
  /**
   * Visit a parse tree produced by the {@code statementDefault}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitStatementDefault(SqlBaseParser.StatementDefaultContext ctx);
  /**
   * Visit a parse tree produced by the {@code explain}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitExplain(SqlBaseParser.ExplainContext ctx);
  /**
   * Visit a parse tree produced by the {@code debug}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitDebug(SqlBaseParser.DebugContext ctx);
  /**
   * Visit a parse tree produced by the {@code showTables}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitShowTables(SqlBaseParser.ShowTablesContext ctx);
  /**
   * Visit a parse tree produced by the {@code showColumns}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitShowColumns(SqlBaseParser.ShowColumnsContext ctx);
  /**
   * Visit a parse tree produced by the {@code showFunctions}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitShowFunctions(SqlBaseParser.ShowFunctionsContext ctx);
  /**
   * Visit a parse tree produced by the {@code showSchemas}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitShowSchemas(SqlBaseParser.ShowSchemasContext ctx);
  /**
   * Visit a parse tree produced by the {@code sysCatalogs}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSysCatalogs(SqlBaseParser.SysCatalogsContext ctx);
  /**
   * Visit a parse tree produced by the {@code sysTables}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSysTables(SqlBaseParser.SysTablesContext ctx);
  /**
   * Visit a parse tree produced by the {@code sysColumns}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSysColumns(SqlBaseParser.SysColumnsContext ctx);
  /**
   * Visit a parse tree produced by the {@code sysTypes}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSysTypes(SqlBaseParser.SysTypesContext ctx);
  /**
   * Visit a parse tree produced by the {@code sysTableTypes}
   * labeled alternative in {@link SqlBaseParser#statement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSysTableTypes(SqlBaseParser.SysTableTypesContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#query}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitQuery(SqlBaseParser.QueryContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#queryNoWith}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitQueryNoWith(SqlBaseParser.QueryNoWithContext ctx);
  /**
   * Visit a parse tree produced by the {@code queryPrimaryDefault}
   * labeled alternative in {@link SqlBaseParser#queryTerm}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitQueryPrimaryDefault(SqlBaseParser.QueryPrimaryDefaultContext ctx);
  /**
   * Visit a parse tree produced by the {@code subquery}
   * labeled alternative in {@link SqlBaseParser#queryTerm}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSubquery(SqlBaseParser.SubqueryContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#orderBy}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitOrderBy(SqlBaseParser.OrderByContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#querySpecification}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitQuerySpecification(SqlBaseParser.QuerySpecificationContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#fromClause}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitFromClause(SqlBaseParser.FromClauseContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#groupBy}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitGroupBy(SqlBaseParser.GroupByContext ctx);
  /**
   * Visit a parse tree produced by the {@code singleGroupingSet}
   * labeled alternative in {@link SqlBaseParser#groupingElement}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSingleGroupingSet(SqlBaseParser.SingleGroupingSetContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#groupingExpressions}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitGroupingExpressions(SqlBaseParser.GroupingExpressionsContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#namedQuery}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitNamedQuery(SqlBaseParser.NamedQueryContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#setQuantifier}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSetQuantifier(SqlBaseParser.SetQuantifierContext ctx);
  /**
   * Visit a parse tree produced by the {@code selectExpression}
   * labeled alternative in {@link SqlBaseParser#selectItem}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSelectExpression(SqlBaseParser.SelectExpressionContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#relation}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitRelation(SqlBaseParser.RelationContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#joinRelation}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitJoinRelation(SqlBaseParser.JoinRelationContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#joinType}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitJoinType(SqlBaseParser.JoinTypeContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#joinCriteria}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitJoinCriteria(SqlBaseParser.JoinCriteriaContext ctx);
  /**
   * Visit a parse tree produced by the {@code tableName}
   * labeled alternative in {@link SqlBaseParser#relationPrimary}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitTableName(SqlBaseParser.TableNameContext ctx);
  /**
   * Visit a parse tree produced by the {@code aliasedQuery}
   * labeled alternative in {@link SqlBaseParser#relationPrimary}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitAliasedQuery(SqlBaseParser.AliasedQueryContext ctx);
  /**
   * Visit a parse tree produced by the {@code aliasedRelation}
   * labeled alternative in {@link SqlBaseParser#relationPrimary}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitAliasedRelation(SqlBaseParser.AliasedRelationContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#expression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitExpression(SqlBaseParser.ExpressionContext ctx);
  /**
   * Visit a parse tree produced by the {@code logicalNot}
   * labeled alternative in {@link SqlBaseParser#booleanExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitLogicalNot(SqlBaseParser.LogicalNotContext ctx);
  /**
   * Visit a parse tree produced by the {@code stringQuery}
   * labeled alternative in {@link SqlBaseParser#booleanExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitStringQuery(SqlBaseParser.StringQueryContext ctx);
  /**
   * Visit a parse tree produced by the {@code booleanDefault}
   * labeled alternative in {@link SqlBaseParser#booleanExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitBooleanDefault(SqlBaseParser.BooleanDefaultContext ctx);
  /**
   * Visit a parse tree produced by the {@code exists}
   * labeled alternative in {@link SqlBaseParser#booleanExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitExists(SqlBaseParser.ExistsContext ctx);
  /**
   * Visit a parse tree produced by the {@code multiMatchQuery}
   * labeled alternative in {@link SqlBaseParser#booleanExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitMultiMatchQuery(SqlBaseParser.MultiMatchQueryContext ctx);
  /**
   * Visit a parse tree produced by the {@code matchQuery}
   * labeled alternative in {@link SqlBaseParser#booleanExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitMatchQuery(SqlBaseParser.MatchQueryContext ctx);
  /**
   * Visit a parse tree produced by the {@code logicalBinary}
   * labeled alternative in {@link SqlBaseParser#booleanExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitLogicalBinary(SqlBaseParser.LogicalBinaryContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#predicated}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitPredicated(SqlBaseParser.PredicatedContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#predicate}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitPredicate(SqlBaseParser.PredicateContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#pattern}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitPattern(SqlBaseParser.PatternContext ctx);
  /**
   * Visit a parse tree produced by the {@code valueExpressionDefault}
   * labeled alternative in {@link SqlBaseParser#valueExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitValueExpressionDefault(SqlBaseParser.ValueExpressionDefaultContext ctx);
  /**
   * Visit a parse tree produced by the {@code comparison}
   * labeled alternative in {@link SqlBaseParser#valueExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitComparison(SqlBaseParser.ComparisonContext ctx);
  /**
   * Visit a parse tree produced by the {@code arithmeticBinary}
   * labeled alternative in {@link SqlBaseParser#valueExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitArithmeticBinary(SqlBaseParser.ArithmeticBinaryContext ctx);
  /**
   * Visit a parse tree produced by the {@code arithmeticUnary}
   * labeled alternative in {@link SqlBaseParser#valueExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitArithmeticUnary(SqlBaseParser.ArithmeticUnaryContext ctx);
  /**
   * Visit a parse tree produced by the {@code cast}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitCast(SqlBaseParser.CastContext ctx);
  /**
   * Visit a parse tree produced by the {@code extract}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitExtract(SqlBaseParser.ExtractContext ctx);
  /**
   * Visit a parse tree produced by the {@code constantDefault}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitConstantDefault(SqlBaseParser.ConstantDefaultContext ctx);
  /**
   * Visit a parse tree produced by the {@code star}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitStar(SqlBaseParser.StarContext ctx);
  /**
   * Visit a parse tree produced by the {@code functionCall}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitFunctionCall(SqlBaseParser.FunctionCallContext ctx);
  /**
   * Visit a parse tree produced by the {@code subqueryExpression}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitSubqueryExpression(SqlBaseParser.SubqueryExpressionContext ctx);
  /**
   * Visit a parse tree produced by the {@code columnReference}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitColumnReference(SqlBaseParser.ColumnReferenceContext ctx);
  /**
   * Visit a parse tree produced by the {@code dereference}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitDereference(SqlBaseParser.DereferenceContext ctx);
  /**
   * Visit a parse tree produced by the {@code parenthesizedExpression}
   * labeled alternative in {@link SqlBaseParser#primaryExpression}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitParenthesizedExpression(SqlBaseParser.ParenthesizedExpressionContext ctx);
  /**
   * Visit a parse tree produced by the {@code nullLiteral}
   * labeled alternative in {@link SqlBaseParser#constant}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitNullLiteral(SqlBaseParser.NullLiteralContext ctx);
  /**
   * Visit a parse tree produced by the {@code numericLiteral}
   * labeled alternative in {@link SqlBaseParser#constant}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitNumericLiteral(SqlBaseParser.NumericLiteralContext ctx);
  /**
   * Visit a parse tree produced by the {@code booleanLiteral}
   * labeled alternative in {@link SqlBaseParser#constant}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitBooleanLiteral(SqlBaseParser.BooleanLiteralContext ctx);
  /**
   * Visit a parse tree produced by the {@code stringLiteral}
   * labeled alternative in {@link SqlBaseParser#constant}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitStringLiteral(SqlBaseParser.StringLiteralContext ctx);
  /**
   * Visit a parse tree produced by the {@code paramLiteral}
   * labeled alternative in {@link SqlBaseParser#constant}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitParamLiteral(SqlBaseParser.ParamLiteralContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#comparisonOperator}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitComparisonOperator(SqlBaseParser.ComparisonOperatorContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#booleanValue}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitBooleanValue(SqlBaseParser.BooleanValueContext ctx);
  /**
   * Visit a parse tree produced by the {@code primitiveDataType}
   * labeled alternative in {@link SqlBaseParser#dataType}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitPrimitiveDataType(SqlBaseParser.PrimitiveDataTypeContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#qualifiedName}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitQualifiedName(SqlBaseParser.QualifiedNameContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#identifier}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitIdentifier(SqlBaseParser.IdentifierContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#tableIdentifier}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitTableIdentifier(SqlBaseParser.TableIdentifierContext ctx);
  /**
   * Visit a parse tree produced by the {@code quotedIdentifier}
   * labeled alternative in {@link SqlBaseParser#quoteIdentifier}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitQuotedIdentifier(SqlBaseParser.QuotedIdentifierContext ctx);
  /**
   * Visit a parse tree produced by the {@code backQuotedIdentifier}
   * labeled alternative in {@link SqlBaseParser#quoteIdentifier}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitBackQuotedIdentifier(SqlBaseParser.BackQuotedIdentifierContext ctx);
  /**
   * Visit a parse tree produced by the {@code unquotedIdentifier}
   * labeled alternative in {@link SqlBaseParser#unquoteIdentifier}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitUnquotedIdentifier(SqlBaseParser.UnquotedIdentifierContext ctx);
  /**
   * Visit a parse tree produced by the {@code digitIdentifier}
   * labeled alternative in {@link SqlBaseParser#unquoteIdentifier}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitDigitIdentifier(SqlBaseParser.DigitIdentifierContext ctx);
  /**
   * Visit a parse tree produced by the {@code decimalLiteral}
   * labeled alternative in {@link SqlBaseParser#number}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitDecimalLiteral(SqlBaseParser.DecimalLiteralContext ctx);
  /**
   * Visit a parse tree produced by the {@code integerLiteral}
   * labeled alternative in {@link SqlBaseParser#number}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitIntegerLiteral(SqlBaseParser.IntegerLiteralContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#string}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitString(SqlBaseParser.StringContext ctx);
  /**
   * Visit a parse tree produced by {@link SqlBaseParser#nonReserved}.
   * @param ctx the parse tree
   * @return the visitor result
   */
  T visitNonReserved(SqlBaseParser.NonReservedContext ctx);
}
