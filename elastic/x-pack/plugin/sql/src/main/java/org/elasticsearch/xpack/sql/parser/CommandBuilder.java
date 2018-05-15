/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.sql.parser;

import org.antlr.v4.runtime.Token;
import org.elasticsearch.common.Booleans;
import org.elasticsearch.xpack.sql.analysis.index.IndexResolver.IndexType;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.DebugContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.ExplainContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.ShowColumnsContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.ShowFunctionsContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.ShowSchemasContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.ShowTablesContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.StringContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.SysCatalogsContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.SysColumnsContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.SysTableTypesContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.SysTablesContext;
import org.elasticsearch.xpack.sql.parser.SqlBaseParser.SysTypesContext;
import org.elasticsearch.xpack.sql.plan.TableIdentifier;
import org.elasticsearch.xpack.sql.plan.logical.command.Command;
import org.elasticsearch.xpack.sql.plan.logical.command.Debug;
import org.elasticsearch.xpack.sql.plan.logical.command.Explain;
import org.elasticsearch.xpack.sql.plan.logical.command.ShowColumns;
import org.elasticsearch.xpack.sql.plan.logical.command.ShowFunctions;
import org.elasticsearch.xpack.sql.plan.logical.command.ShowSchemas;
import org.elasticsearch.xpack.sql.plan.logical.command.ShowTables;
import org.elasticsearch.xpack.sql.plan.logical.command.sys.SysCatalogs;
import org.elasticsearch.xpack.sql.plan.logical.command.sys.SysColumns;
import org.elasticsearch.xpack.sql.plan.logical.command.sys.SysTableTypes;
import org.elasticsearch.xpack.sql.plan.logical.command.sys.SysTables;
import org.elasticsearch.xpack.sql.plan.logical.command.sys.SysTypes;
import org.elasticsearch.xpack.sql.proto.SqlTypedParamValue;
import org.elasticsearch.xpack.sql.tree.Location;
import org.elasticsearch.xpack.sql.util.StringUtils;

import java.util.ArrayList;
import java.util.EnumSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;

abstract class CommandBuilder extends LogicalPlanBuilder {

    protected CommandBuilder(Map<Token, SqlTypedParamValue> params) {
        super(params);
    }

    @Override
    public Command visitDebug(DebugContext ctx) {
        Location loc = source(ctx);
        if (ctx.FORMAT().size() > 1) {
            throw new ParsingException(loc, "Debug FORMAT should be specified at most once");
        }
        if (ctx.PLAN().size() > 1) {
            throw new ParsingException(loc, "Debug PLAN should be specified at most once");
        }

        Debug.Type type = null;

        if (ctx.type != null) {
            if (ctx.type.getType() == SqlBaseLexer.ANALYZED) {
                type = Debug.Type.ANALYZED;
            }
            else {
                type = Debug.Type.OPTIMIZED;
            }
        }
        boolean graphViz = ctx.format != null && ctx.format.getType() == SqlBaseLexer.GRAPHVIZ;
        Debug.Format format = graphViz ? Debug.Format.GRAPHVIZ : Debug.Format.TEXT;

        return new Debug(loc, plan(ctx.statement()), type, format);
    }


    @Override
    public Command visitExplain(ExplainContext ctx) {
        Location loc = source(ctx);
        if (ctx.PLAN().size() > 1) {
            throw new ParsingException(loc, "Explain TYPE should be specified at most once");
        }
        if (ctx.FORMAT().size() > 1) {
            throw new ParsingException(loc, "Explain FORMAT should be specified at most once");
        }
        if (ctx.VERIFY().size() > 1) {
            throw new ParsingException(loc, "Explain VERIFY should be specified at most once");
        }

        Explain.Type type = null;

        if (ctx.type != null) {
            switch (ctx.type.getType()) {
                case SqlBaseLexer.PARSED:
                    type = Explain.Type.PARSED;
                    break;
                case SqlBaseLexer.ANALYZED:
                    type = Explain.Type.ANALYZED;
                    break;
                case SqlBaseLexer.OPTIMIZED:
                    type = Explain.Type.OPTIMIZED;
                    break;
                case SqlBaseLexer.MAPPED:
                    type = Explain.Type.MAPPED;
                    break;
                case SqlBaseLexer.EXECUTABLE:
                    type = Explain.Type.EXECUTABLE;
                    break;
                default:
                    type = Explain.Type.ALL;
            }
        }
        boolean graphViz = ctx.format != null && ctx.format.getType() == SqlBaseLexer.GRAPHVIZ;
        Explain.Format format = graphViz ? Explain.Format.GRAPHVIZ : Explain.Format.TEXT;
        boolean verify = (ctx.verify != null ? Booleans.parseBoolean(ctx.verify.getText().toLowerCase(Locale.ROOT), true) : true);

        return new Explain(loc, plan(ctx.statement()), type, format, verify);
    }

    @Override
    public Object visitShowFunctions(ShowFunctionsContext ctx) {
        return new ShowFunctions(source(ctx), visitPattern(ctx.pattern()));
    }

    @Override
    public Object visitShowTables(ShowTablesContext ctx) {
        return new ShowTables(source(ctx), visitPattern(ctx.pattern()));
    }

    @Override
    public Object visitShowSchemas(ShowSchemasContext ctx) {
        return new ShowSchemas(source(ctx));
    }

    @Override
    public Object visitShowColumns(ShowColumnsContext ctx) {
        TableIdentifier identifier = visitTableIdentifier(ctx.tableIdentifier());
        return new ShowColumns(source(ctx), identifier.index());
    }

    @Override
    public Object visitSysCatalogs(SysCatalogsContext ctx) {
        return new SysCatalogs(source(ctx));
    }

    @Override
    public SysTables visitSysTables(SysTablesContext ctx) {
        List<IndexType> types = new ArrayList<>();
        boolean legacyTableType = false;
        for (StringContext string : ctx.string()) {
            String value = string(string);
            if (value != null) {
                // check special ODBC wildcard case
                if (value.equals(StringUtils.SQL_WILDCARD) && ctx.string().size() == 1) {
                    // since % is the same as not specifying a value, choose
                    // https://docs.microsoft.com/en-us/sql/odbc/reference/develop-app/value-list-arguments?view=ssdt-18vs2017
                    // that is skip the value
                }
                // special case for legacy apps (like msquery) that always asks for 'TABLE'
                // which we manually map to all concrete tables supported
                else if (value.toUpperCase(Locale.ROOT).equals("TABLE")) {
                    legacyTableType = true;
                    types.add(IndexType.INDEX);
                } else {
                    IndexType type = IndexType.from(value);
                    types.add(type);
                }
            }
        }

        // if the ODBC enumeration is specified, skip validation
        EnumSet<IndexType> set = types.isEmpty() ? null : EnumSet.copyOf(types);
        return new SysTables(source(ctx), visitPattern(ctx.clusterPattern), visitPattern(ctx.tablePattern), set, legacyTableType);
    }

    @Override
    public Object visitSysColumns(SysColumnsContext ctx) {
        Location loc = source(ctx);
        return new SysColumns(loc, string(ctx.cluster), visitPattern(ctx.indexPattern), visitPattern(ctx.columnPattern));
    }

    @Override
    public SysTypes visitSysTypes(SysTypesContext ctx) {
        return new SysTypes(source(ctx));
    }

    @Override
    public Object visitSysTableTypes(SysTableTypesContext ctx) {
        return new SysTableTypes(source(ctx));
    }
}
