/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */
package org.elasticsearch.xpack.watcher.common.text;

import org.elasticsearch.common.bytes.BytesReference;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.common.xcontent.json.JsonXContent;
import org.elasticsearch.script.Script;
import org.elasticsearch.script.ScriptService;
import org.elasticsearch.script.ScriptType;
import org.elasticsearch.script.TemplateScript;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.watcher.Watcher;
import org.junit.Before;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

import static java.util.Collections.singletonMap;
import static java.util.Collections.unmodifiableMap;
import static org.elasticsearch.common.xcontent.XContentFactory.jsonBuilder;
import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.hamcrest.Matchers.nullValue;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class TextTemplateTests extends ESTestCase {

    private ScriptService service;
    private TextTemplateEngine engine;
    private final String lang = "mustache";

    @Before
    public void init() throws Exception {
        service = mock(ScriptService.class);
        engine = new TextTemplateEngine(Settings.EMPTY, service);
    }

    public void testRender() throws Exception {
        String templateText = "_template";
        Map<String, Object> params = singletonMap("param_key", "param_val");
        Map<String, Object> model = singletonMap("model_key", "model_val");
        Map<String, Object> merged = new HashMap<>(params);
        merged.putAll(model);
        merged = unmodifiableMap(merged);
        ScriptType type = randomFrom(ScriptType.values());

        TemplateScript.Factory compiledTemplate = templateParams ->
                new TemplateScript(templateParams) {
                    @Override
                    public String execute() {
                        return "rendered_text";
                    }
                };

        when(service.compile(new Script(type, type == ScriptType.STORED ? null : lang, templateText,
                type == ScriptType.INLINE ? Collections.singletonMap("content_type", "text/plain") : null,
                merged), Watcher.SCRIPT_TEMPLATE_CONTEXT)).thenReturn(compiledTemplate);

        TextTemplate template = templateBuilder(type, templateText, params);
        assertThat(engine.render(template, model), is("rendered_text"));
    }

    public void testRenderOverridingModel() throws Exception {
        String templateText = "_template";
        Map<String, Object> params = singletonMap("key", "param_val");
        Map<String, Object> model = singletonMap("key", "model_val");
        ScriptType type = randomFrom(ScriptType.values());

        TemplateScript.Factory compiledTemplate = templateParams ->
            new TemplateScript(templateParams) {
                @Override
                public String execute() {
                    return "rendered_text";
                }
            };

        when(service.compile(new Script(type, type == ScriptType.STORED ? null : lang, templateText,
                type == ScriptType.INLINE ? Collections.singletonMap("content_type", "text/plain") : null,
                model), Watcher.SCRIPT_TEMPLATE_CONTEXT)).thenReturn(compiledTemplate);

        TextTemplate template = templateBuilder(type, templateText, params);
        assertThat(engine.render(template, model), is("rendered_text"));
    }

    public void testRenderDefaults() throws Exception {
        String templateText = "_template";
        Map<String, Object> model = singletonMap("key", "model_val");

        TemplateScript.Factory compiledTemplate = templateParams ->
            new TemplateScript(templateParams) {
                @Override
                public String execute() {
                    return "rendered_text";
                }
            };

        when(service.compile(new Script(ScriptType.INLINE, lang, templateText,
                Collections.singletonMap("content_type", "text/plain"), model), Watcher.SCRIPT_TEMPLATE_CONTEXT))
                .thenReturn(compiledTemplate);

        TextTemplate template = new TextTemplate(templateText);
        assertThat(engine.render(template, model), is("rendered_text"));
    }

    public void testParser() throws Exception {
        ScriptType type = randomScriptType();
        TextTemplate template =
                templateBuilder(type, "_template", singletonMap("param_key", "param_val"));
        XContentBuilder builder = jsonBuilder().startObject();
        switch (type) {
            case INLINE:
                builder.field("source", template.getTemplate());
                break;
            case STORED:
                builder.field("id", template.getTemplate());
        }
        builder.field("params", template.getParams());
        builder.endObject();
        BytesReference bytes = BytesReference.bytes(builder);
        XContentParser parser = createParser(JsonXContent.jsonXContent, bytes);
        parser.nextToken();
        TextTemplate parsed = TextTemplate.parse(parser);
        assertThat(parsed, notNullValue());
        assertThat(parsed, equalTo(template));
    }

    public void testParserParserSelfGenerated() throws Exception {
        ScriptType type = randomScriptType();
        TextTemplate template =
                templateBuilder(type, "_template", singletonMap("param_key", "param_val"));

        XContentBuilder builder = jsonBuilder().value(template);
        BytesReference bytes = BytesReference.bytes(builder);
        XContentParser parser = createParser(JsonXContent.jsonXContent, bytes);
        parser.nextToken();
        TextTemplate parsed = TextTemplate.parse(parser);
        assertThat(parsed, notNullValue());
        assertThat(parsed, equalTo(template));
    }

    public void testParserInvalidUnexpectedField() throws Exception {
        XContentBuilder builder = jsonBuilder().startObject()
                .field("unknown_field", "value")
                .endObject();
        BytesReference bytes = BytesReference.bytes(builder);
        XContentParser parser = createParser(JsonXContent.jsonXContent, bytes);
        parser.nextToken();
        try {
            TextTemplate.parse(parser);
            fail("expected parse exception when encountering an unknown field");
        } catch (IllegalArgumentException e) {
            assertThat(e.getMessage(), containsString("[script] unknown field [unknown_field], parser not found"));
        }
    }

    public void testParserInvalidUnknownScriptType() throws Exception {
        XContentBuilder builder = jsonBuilder().startObject()
                .field("template", "_template")
                .field("type", "unknown_type")
                .startObject("params").endObject()
                .endObject();
        BytesReference bytes = BytesReference.bytes(builder);
        XContentParser parser = createParser(JsonXContent.jsonXContent, bytes);
        parser.nextToken();
        try {
            TextTemplate.parse(parser);
            fail("expected parse exception when script type is unknown");
        } catch (IllegalArgumentException e) {
            assertThat(e.getMessage(), is("[script] unknown field [template], parser not found"));
        }
    }

    public void testParserInvalidMissingText() throws Exception {
        XContentBuilder builder = jsonBuilder().startObject()
                .field("type", ScriptType.STORED)
                .startObject("params").endObject()
                .endObject();
        BytesReference bytes = BytesReference.bytes(builder);
        XContentParser parser = createParser(JsonXContent.jsonXContent, bytes);
        parser.nextToken();
        try {
            TextTemplate.parse(parser);
            fail("expected parse exception when template text is missing");
        } catch (IllegalArgumentException e) {
            assertThat(e.getMessage(), containsString("[script] unknown field [type], parser not found"));
        }
    }

    public void testNullObject() throws Exception {
        assertThat(engine.render(null ,new HashMap<>()), is(nullValue()));
    }

    private TextTemplate templateBuilder(ScriptType type, String text, Map<String, Object> params) {
        return new TextTemplate(text, null, type, params);
    }

    private static ScriptType randomScriptType() {
        return randomFrom(ScriptType.values());
    }
}
