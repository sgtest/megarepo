/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0 and the Server Side Public License, v 1; you may not use this file except
 * in compliance with, at your election, the Elastic License 2.0 or the Server
 * Side Public License, v 1.
 */

package org.elasticsearch.painless;

import org.elasticsearch.common.ParseField;
import org.elasticsearch.common.xcontent.ToXContentObject;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.painless.action.PainlessContextClassBindingInfo;
import org.elasticsearch.painless.action.PainlessContextClassInfo;
import org.elasticsearch.painless.action.PainlessContextConstructorInfo;
import org.elasticsearch.painless.action.PainlessContextFieldInfo;
import org.elasticsearch.painless.action.PainlessContextInfo;
import org.elasticsearch.painless.action.PainlessContextInstanceBindingInfo;
import org.elasticsearch.painless.action.PainlessContextMethodInfo;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;

public class PainlessInfoJson {
    public static class Context implements ToXContentObject {
        private final String name;
        private final List<Class> classes;
        private final List<Method> importedMethods;
        private final List<PainlessContextClassBindingInfo> classBindings;
        private final List<PainlessContextInstanceBindingInfo> instanceBindings;

        public Context(
                PainlessContextInfo info,
                Set<PainlessContextClassInfo> commonClassInfos,
                Map<String, String> javaNamesToDisplayNames,
                StdlibJavadocExtractor extractor
        ) throws IOException {
            this.name = info.getName();
            List<PainlessContextClassInfo> classInfos = ContextGeneratorCommon.excludeCommonClassInfos(commonClassInfos, info.getClasses());
            classInfos = ContextGeneratorCommon.sortClassInfos(classInfos);
            this.classes = Class.fromInfos(classInfos, javaNamesToDisplayNames, extractor);
            // TODO(stu): should we use extractor for these imported methods?
            this.importedMethods = Method.fromInfos(info.getImportedMethods(), javaNamesToDisplayNames);
            this.classBindings = info.getClassBindings();
            this.instanceBindings = info.getInstanceBindings();
        }

        public Context(
            PainlessContextInfo info,
            Set<PainlessContextClassInfo> commonClassInfos,
            Map<String, String> javaNamesToDisplayNames
        ) {
            this.name = info.getName();
            List<PainlessContextClassInfo> classInfos = ContextGeneratorCommon.excludeCommonClassInfos(commonClassInfos, info.getClasses());
            classInfos = ContextGeneratorCommon.sortClassInfos(classInfos);
            this.classes = Class.fromInfos(classInfos, javaNamesToDisplayNames);
            this.importedMethods = Method.fromInfos(info.getImportedMethods(), javaNamesToDisplayNames);
            this.classBindings = info.getClassBindings();
            this.instanceBindings = info.getInstanceBindings();
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(PainlessContextInfo.NAME.getPreferredName(), name);
            builder.field(PainlessContextInfo.CLASSES.getPreferredName(), classes);
            builder.field(PainlessContextInfo.IMPORTED_METHODS.getPreferredName(), importedMethods);
            builder.field(PainlessContextInfo.CLASS_BINDINGS.getPreferredName(), classBindings);
            builder.field(PainlessContextInfo.INSTANCE_BINDINGS.getPreferredName(), instanceBindings);
            builder.endObject();

            return builder;
        }

        public String getName() {
            return name;
        }
    }

    public static class Class implements ToXContentObject {
        private final String name;
        private final boolean imported;
        private final List<Constructor> constructors;
        private final List<Method> staticMethods;
        private final List<Method> methods;
        private final List<Field> staticFields;
        private final List<Field> fields;

        private Class(
                String name,
                boolean imported,
                List<Constructor> constructors,
                List<Method> staticMethods,
                List<Method> methods,
                List<Field> staticFields,
                List<Field> fields
        ) {
            this.name = name;
            this.imported = imported;
            this.constructors = constructors;
            this.staticMethods = staticMethods;
            this.methods = methods;
            this.staticFields = staticFields;
            this.fields = fields;
        }

        public static List<Class> fromInfos(
            List<PainlessContextClassInfo> infos,
            Map<String, String> javaNamesToDisplayNames,
            StdlibJavadocExtractor extractor
        ) throws IOException {
            List<Class> classes = new ArrayList<>(infos.size());
            for (PainlessContextClassInfo info : infos) {
                StdlibJavadocExtractor.ParsedJavaClass parsedClass = extractor.parseClass(info.getName());
                classes.add(new Class(
                        javaNamesToDisplayNames.get(info.getName()),
                        info.isImported(),
                        Constructor.fromInfos(info.getConstructors(), javaNamesToDisplayNames, parsedClass),
                        Method.fromInfos(info.getStaticMethods(), javaNamesToDisplayNames, parsedClass),
                        Method.fromInfos(info.getMethods(), javaNamesToDisplayNames, parsedClass),
                        Field.fromInfos(info.getStaticFields(), javaNamesToDisplayNames, parsedClass),
                        Field.fromInfos(info.getFields(), javaNamesToDisplayNames, parsedClass)
                ));
            }
            return classes;
        }

        public static List<Class> fromInfos(List<PainlessContextClassInfo> infos, Map<String, String> javaNamesToDisplayNames) {
            List<Class> classes = new ArrayList<>(infos.size());
            for (PainlessContextClassInfo info : infos) {
                classes.add(new Class(
                        javaNamesToDisplayNames.get(info.getName()),
                        info.isImported(),
                        Constructor.fromInfos(info.getConstructors(), javaNamesToDisplayNames),
                        Method.fromInfos(info.getStaticMethods(), javaNamesToDisplayNames),
                        Method.fromInfos(info.getMethods(), javaNamesToDisplayNames),
                        Field.fromInfos(info.getStaticFields(), javaNamesToDisplayNames),
                        Field.fromInfos(info.getFields(), javaNamesToDisplayNames)
                ));
            }
            return classes;
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(PainlessContextClassInfo.NAME.getPreferredName(), name);
            builder.field(PainlessContextClassInfo.IMPORTED.getPreferredName(), imported);
            builder.field(PainlessContextClassInfo.CONSTRUCTORS.getPreferredName(), constructors);
            builder.field(PainlessContextClassInfo.STATIC_METHODS.getPreferredName(), staticMethods);
            builder.field(PainlessContextClassInfo.METHODS.getPreferredName(), methods);
            builder.field(PainlessContextClassInfo.STATIC_FIELDS.getPreferredName(), staticFields);
            builder.field(PainlessContextClassInfo.FIELDS.getPreferredName(), fields);
            builder.endObject();

            return builder;
        }
    }

    private static List<String> toDisplayParameterTypes(List<String> rawParameterTypes, Map<String, String> javaNamesToDisplayNames) {
        List<String> displayParameterTypes = new ArrayList<>(rawParameterTypes.size());
        for (String rawParameterType: rawParameterTypes) {
            displayParameterTypes.add(ContextGeneratorCommon.getType(javaNamesToDisplayNames, rawParameterType));
        }
        return displayParameterTypes;
    }

    public static class Method implements ToXContentObject {
        private final String declaring;
        private final String name;
        private final String rtn;
        private final String javadoc;
        private final List<String> parameters;
        private final List<String> parameterNames;
        public static final ParseField PARAMETER_NAMES = new ParseField("parameter_names");
        public static final ParseField JAVADOC = new ParseField("javadoc");

        private Method(String declaring, String name, String rtn, String javadoc, List<String> parameters, List<String> parameterNames) {
            this.declaring = declaring;
            this.name = name;
            this.rtn = rtn;
            this.javadoc = javadoc;
            this.parameters = parameters;
            this.parameterNames = parameterNames;
        }

        public static List<Method> fromInfos(List<PainlessContextMethodInfo> infos, Map<String, String> javaNamesToDisplayNames) {
            List<Method> methods = new ArrayList<>(infos.size());
            for (PainlessContextMethodInfo info: infos) {
                methods.add(new Method(
                        javaNamesToDisplayNames.get(info.getDeclaring()),
                        info.getName(),
                        ContextGeneratorCommon.getType(javaNamesToDisplayNames, info.getRtn()),
                        null,
                        toDisplayParameterTypes(info.getParameters(), javaNamesToDisplayNames),
                        null
                ));
            }
            return methods;
        }

        public static List<Method> fromInfos(
                List<PainlessContextMethodInfo> infos,
                Map<String, String> javaNamesToDisplayNames,
                StdlibJavadocExtractor.ParsedJavaClass parsed
        ) {
            List<Method> methods = new ArrayList<>(infos.size());
            for (PainlessContextMethodInfo info: infos) {
                String javadoc = null;
                List<String> parameterNames = null;

                String name = info.getName();
                List<String> parameterTypes = info.getParameters();

                StdlibJavadocExtractor.ParsedMethod parsedMethod = parsed.getMethod(name, parameterTypes);
                if (parsedMethod != null) {
                    javadoc = parsedMethod.javadoc;
                    parameterNames = parsedMethod.parameterNames;
                }

                methods.add(new Method(
                        javaNamesToDisplayNames.get(info.getDeclaring()),
                        name,
                        ContextGeneratorCommon.getType(javaNamesToDisplayNames, info.getRtn()),
                        javadoc,
                        toDisplayParameterTypes(parameterTypes, javaNamesToDisplayNames),
                        parameterNames
                ));
            }
            return methods;
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(PainlessContextMethodInfo.DECLARING.getPreferredName(), declaring);
            builder.field(PainlessContextMethodInfo.NAME.getPreferredName(), name);
            builder.field(PainlessContextMethodInfo.RTN.getPreferredName(), rtn);
            if (javadoc != null && "".equals(javadoc) == false) {
                builder.field(JAVADOC.getPreferredName(), javadoc);
            }
            builder.field(PainlessContextMethodInfo.PARAMETERS.getPreferredName(), parameters);
            if (parameterNames != null && parameterNames.size() > 0) {
                builder.field(PARAMETER_NAMES.getPreferredName(), parameterNames);
            }
            builder.endObject();

            return builder;
        }
    }

    public static class Constructor implements ToXContentObject {
        private final String declaring;
        private final List<String> parameters;
        private final List<String> parameterNames;
        private final String javadoc;

        public static final ParseField JAVADOC = new ParseField("javadoc");
        public static final ParseField PARAMETER_NAMES = new ParseField("parameter_names");

        private Constructor(String declaring, List<String> parameters, List<String> parameterNames, String javadoc) {
            this.declaring = declaring;
            this.parameters = parameters;
            this.parameterNames = parameterNames;
            this.javadoc = javadoc;
        }

        public static List<Constructor> fromInfos(List<PainlessContextConstructorInfo> infos, Map<String, String> javaNamesToDisplayNames) {
            List<Constructor> constructors = new ArrayList<>(infos.size());
            for (PainlessContextConstructorInfo info: infos) {
                constructors.add(new Constructor(
                        javaNamesToDisplayNames.get(info.getDeclaring()),
                        toDisplayParameterTypes(info.getParameters(), javaNamesToDisplayNames),
                        null,
                        null
                ));
            }
            return constructors;
        }

        private static List<Constructor> fromInfos(
                List<PainlessContextConstructorInfo> infos,
                Map<String, String> javaNamesToDisplayNames,
                StdlibJavadocExtractor.ParsedJavaClass pj
        ) {
            List<Constructor> constructors = new ArrayList<>(infos.size());
            for (PainlessContextConstructorInfo info: infos) {
                List<String> parameterTypes = info.getParameters();
                List<String> parameterNames = null;
                String javadoc = null;

                StdlibJavadocExtractor.ParsedMethod parsed = pj.getConstructor(parameterTypes);
                if (parsed != null) {
                    parameterNames = parsed.parameterNames;
                    javadoc = parsed.javadoc;
                }

                constructors.add(new Constructor(
                        javaNamesToDisplayNames.get(info.getDeclaring()),
                        toDisplayParameterTypes(parameterTypes, javaNamesToDisplayNames),
                        parameterNames,
                        javadoc
                ));
            }
            return constructors;
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(PainlessContextConstructorInfo.DECLARING.getPreferredName(), declaring);
            builder.field(PainlessContextConstructorInfo.PARAMETERS.getPreferredName(), parameters);
            if (parameterNames != null && parameterNames.size() > 0) {
                builder.field(PARAMETER_NAMES.getPreferredName(), parameterNames);
            }
            if (javadoc != null && "".equals(javadoc) == false) {
                builder.field(JAVADOC.getPreferredName(), javadoc);
            }
            builder.endObject();

            return builder;
        }
    }

    public static class Field implements ToXContentObject {
        private final String declaring;
        private final String name;
        private final String type;
        private final String javadoc;

        public static final ParseField JAVADOC = new ParseField("javadoc");

        private Field(String declaring, String name, String type, String javadoc) {
            this.declaring = declaring;
            this.name = name;
            this.type = type;
            this.javadoc = javadoc;
        }

        public static List<Field> fromInfos(List<PainlessContextFieldInfo> infos, Map<String, String> javaNamesToDisplayNames) {
            List<Field> fields = new ArrayList<>(infos.size());
            for (PainlessContextFieldInfo info: infos) {
                fields.add(new Field(
                        javaNamesToDisplayNames.get(info.getDeclaring()),
                        info.getName(),
                        ContextGeneratorCommon.getType(javaNamesToDisplayNames, info.getType()),
                        null
                ));
            }
            return fields;
        }

        public static List<Field> fromInfos(
                List<PainlessContextFieldInfo> infos,
                Map<String, String> javaNamesToDisplayNames,
                StdlibJavadocExtractor.ParsedJavaClass pj
        ) {
            List<Field> fields = new ArrayList<>(infos.size());
            for (PainlessContextFieldInfo info: infos) {
                String name = info.getName();
                fields.add(new Field(
                        javaNamesToDisplayNames.get(info.getDeclaring()),
                        name,
                        ContextGeneratorCommon.getType(javaNamesToDisplayNames, info.getType()),
                        pj.getField(name)
                ));
            }
            return fields;
        }

        @Override
        public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
            builder.startObject();
            builder.field(PainlessContextFieldInfo.DECLARING.getPreferredName(), declaring);
            builder.field(PainlessContextFieldInfo.NAME.getPreferredName(), name);
            builder.field(PainlessContextFieldInfo.TYPE.getPreferredName(), type);
            if (javadoc != null && "".equals(javadoc) == false) {
                builder.field(JAVADOC.getPreferredName(), javadoc);
            }
            builder.endObject();

            return builder;
        }
    }
}
