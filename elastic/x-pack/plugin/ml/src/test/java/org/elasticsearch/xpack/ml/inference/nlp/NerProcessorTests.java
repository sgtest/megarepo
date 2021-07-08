/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.nlp;

import org.elasticsearch.common.ValidationException;
import org.elasticsearch.test.ESTestCase;
import org.elasticsearch.xpack.core.ml.inference.results.NerResults;
import org.elasticsearch.xpack.ml.inference.deployment.PyTorchResult;
import org.elasticsearch.xpack.ml.inference.nlp.tokenizers.BertTokenizer;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.stream.Collectors;

import static org.hamcrest.Matchers.containsString;
import static org.hamcrest.Matchers.empty;
import static org.hamcrest.Matchers.equalTo;
import static org.hamcrest.Matchers.hasSize;
import static org.hamcrest.Matchers.is;
import static org.mockito.Mockito.mock;

public class NerProcessorTests extends ESTestCase {

    public void testBuildIobMap_WithDefault() {
        NerProcessor.IobTag[] map = NerProcessor.buildIobMap(randomBoolean() ? null : Collections.emptyList());
        for (int i=0; i<map.length; i++) {
            assertEquals(i, map[i].ordinal());
        }
    }

    public void testBuildIobMap_Reordered() {
        NerProcessor.IobTag[] tags = new NerProcessor.IobTag[]{
            NerProcessor.IobTag.I_MISC,
            NerProcessor.IobTag.O,
            NerProcessor.IobTag.B_MISC,
            NerProcessor.IobTag.I_PER
        };

        List<String> classLabels = Arrays.stream(tags).map(NerProcessor.IobTag::toString).collect(Collectors.toList());
        NerProcessor.IobTag[] map = NerProcessor.buildIobMap(classLabels);
        for (int i=0; i<map.length; i++) {
            assertNotEquals(i, map[i].ordinal());
        }
        assertArrayEquals(tags, map);
    }

    public void testValidate_DuplicateLabels() {
        NerProcessor.IobTag[] tags = new NerProcessor.IobTag[]{
            NerProcessor.IobTag.I_MISC,
            NerProcessor.IobTag.B_MISC,
            NerProcessor.IobTag.B_MISC,
            NerProcessor.IobTag.O,
        };

        List<String> classLabels = Arrays.stream(tags).map(NerProcessor.IobTag::toString).collect(Collectors.toList());
        NlpTaskConfig config = NlpTaskConfig.builder().setClassificationLabels(classLabels).build();

        ValidationException ve = expectThrows(ValidationException.class, () -> new NerProcessor(mock(BertTokenizer.class), config));
        assertThat(ve.getMessage(),
            containsString("the classification label [B_MISC] is duplicated in the list [I_MISC, B_MISC, B_MISC, O]"));
    }

    public void testValidate_NotAEntityLabel() {
        List<String> classLabels = List.of("foo", NerProcessor.IobTag.B_MISC.toString());
        NlpTaskConfig config = NlpTaskConfig.builder().setClassificationLabels(classLabels).build();

        ValidationException ve = expectThrows(ValidationException.class, () -> new NerProcessor(mock(BertTokenizer.class), config));
        assertThat(ve.getMessage(), containsString("classification label [foo] is not an entity I-O-B tag"));
        assertThat(ve.getMessage(),
            containsString("Valid entity I-O-B tags are [O, B_MISC, I_MISC, B_PER, I_PER, B_ORG, I_ORG, B_LOC, I_LOC]"));
    }

    public void testProcessResults_GivenNoTokens() {
        NerProcessor.NerResultProcessor processor = createProcessor(Collections.emptyList(), "");
        NerResults result = (NerResults) processor.processResult(new PyTorchResult("test", null, null));
        assertThat(result.getEntityGroups(), is(empty()));
    }

    public void testProcessResults() {
        NerProcessor.NerResultProcessor processor =
            createProcessor(Arrays.asList("el", "##astic", "##search", "many", "use", "in", "london"), "Many use Elasticsearch in London");
        double[][] scores = {
            { 7, 0, 0, 0, 0, 0, 0, 0, 0}, // many
            { 7, 0, 0, 0, 0, 0, 0, 0, 0}, // use
            { 0.01, 0.01, 0, 0.01, 0, 7, 0, 3, 0}, // el
            { 0.01, 0.01, 0, 0, 0, 0, 0, 0, 0}, // ##astic
            { 0, 0, 0, 0, 0, 0, 0, 0, 0}, // ##search
            { 0, 0, 0, 0, 0, 0, 0, 0, 0}, // in
            { 0, 0, 0, 0, 0, 0, 0, 6, 0} // london
        };
        NerResults result = (NerResults) processor.processResult(new PyTorchResult("1", scores, null));

        assertThat(result.getEntityGroups().size(), equalTo(2));
        assertThat(result.getEntityGroups().get(0).getWord(), equalTo("elasticsearch"));
        assertThat(result.getEntityGroups().get(0).getLabel(), equalTo(NerProcessor.Entity.ORGANISATION.toString()));
        assertThat(result.getEntityGroups().get(1).getWord(), equalTo("london"));
        assertThat(result.getEntityGroups().get(1).getLabel(), equalTo(NerProcessor.Entity.LOCATION.toString()));
    }

    public void testProcessResults_withIobMap() {

        NerProcessor.IobTag [] iobMap = new NerProcessor.IobTag[] {
            NerProcessor.IobTag.B_LOC,
            NerProcessor.IobTag.I_LOC,
            NerProcessor.IobTag.B_MISC,
            NerProcessor.IobTag.I_MISC,
            NerProcessor.IobTag.B_PER,
            NerProcessor.IobTag.I_PER,
            NerProcessor.IobTag.B_ORG,
            NerProcessor.IobTag.I_ORG,
            NerProcessor.IobTag.O
        };

        NerProcessor.NerResultProcessor processor = createProcessor(
            Arrays.asList("el", "##astic", "##search", "many", "use", "in", "london"),
            "Elasticsearch in London",
            iobMap
        );

        double[][] scores = {
            { 0.01, 0.01, 0, 0.01, 0, 0, 7, 3, 0}, // el
            { 0.01, 0.01, 0, 0, 0, 0, 0, 0, 0}, // ##astic
            { 0, 0, 0, 0, 0, 0, 0, 0, 0}, // ##search
            { 0, 0, 0, 0, 0, 0, 0, 0, 5}, // in
            { 6, 0, 0, 0, 0, 0, 0, 0, 0} // london
        };
        NerResults result = (NerResults) processor.processResult(new PyTorchResult("1", scores, null));

        assertThat(result.getEntityGroups().size(), equalTo(2));
        assertThat(result.getEntityGroups().get(0).getWord(), equalTo("elasticsearch"));
        assertThat(result.getEntityGroups().get(0).getLabel(), equalTo(NerProcessor.Entity.ORGANISATION.toString()));
        assertThat(result.getEntityGroups().get(1).getWord(), equalTo("london"));
        assertThat(result.getEntityGroups().get(1).getLabel(), equalTo(NerProcessor.Entity.LOCATION.toString()));
    }

    public void testGroupTaggedTokens() {
        List<NerProcessor.NerResultProcessor.TaggedToken> tokens = new ArrayList<>();
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Hi", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Sarah", NerProcessor.IobTag.B_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Jessica", NerProcessor.IobTag.I_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("I", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("live", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("in", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Manchester", NerProcessor.IobTag.B_LOC, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("and", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("work", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("for", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Elastic", NerProcessor.IobTag.B_ORG, 1.0));

        List<NerResults.EntityGroup> entityGroups = NerProcessor.NerResultProcessor.groupTaggedTokens(tokens);
        assertThat(entityGroups, hasSize(3));
        assertThat(entityGroups.get(0).getLabel(), equalTo("person"));
        assertThat(entityGroups.get(0).getWord(), equalTo("Sarah Jessica"));
        assertThat(entityGroups.get(1).getLabel(), equalTo("location"));
        assertThat(entityGroups.get(1).getWord(), equalTo("Manchester"));
        assertThat(entityGroups.get(2).getLabel(), equalTo("organisation"));
        assertThat(entityGroups.get(2).getWord(), equalTo("Elastic"));
    }

    public void testGroupTaggedTokens_GivenNoEntities() {
        List<NerProcessor.NerResultProcessor.TaggedToken> tokens = new ArrayList<>();
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Hi", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("there", NerProcessor.IobTag.O, 1.0));

        List<NerResults.EntityGroup> entityGroups = NerProcessor.NerResultProcessor.groupTaggedTokens(tokens);
        assertThat(entityGroups, is(empty()));
    }

    public void testGroupTaggedTokens_GivenConsecutiveEntities() {
        List<NerProcessor.NerResultProcessor.TaggedToken> tokens = new ArrayList<>();
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Rita", NerProcessor.IobTag.B_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Sue", NerProcessor.IobTag.B_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("and", NerProcessor.IobTag.O, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("Bob", NerProcessor.IobTag.B_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("to", NerProcessor.IobTag.O, 1.0));

        List<NerResults.EntityGroup> entityGroups = NerProcessor.NerResultProcessor.groupTaggedTokens(tokens);
        assertThat(entityGroups, hasSize(3));
        assertThat(entityGroups.get(0).getLabel(), equalTo("person"));
        assertThat(entityGroups.get(0).getWord(), equalTo("Rita"));
        assertThat(entityGroups.get(1).getLabel(), equalTo("person"));
        assertThat(entityGroups.get(1).getWord(), equalTo("Sue"));
        assertThat(entityGroups.get(2).getLabel(), equalTo("person"));
        assertThat(entityGroups.get(2).getWord(), equalTo("Bob"));
    }

    public void testGroupTaggedTokens_GivenConsecutiveContinuingEntities() {
        List<NerProcessor.NerResultProcessor.TaggedToken> tokens = new ArrayList<>();
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("FirstName", NerProcessor.IobTag.B_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("SecondName", NerProcessor.IobTag.I_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("NextPerson", NerProcessor.IobTag.B_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("NextPersonSecondName", NerProcessor.IobTag.I_PER, 1.0));
        tokens.add(new NerProcessor.NerResultProcessor.TaggedToken("something_else", NerProcessor.IobTag.B_ORG, 1.0));

        List<NerResults.EntityGroup> entityGroups = NerProcessor.NerResultProcessor.groupTaggedTokens(tokens);
        assertThat(entityGroups, hasSize(3));
        assertThat(entityGroups.get(0).getLabel(), equalTo("person"));
        assertThat(entityGroups.get(0).getWord(), equalTo("FirstName SecondName"));
        assertThat(entityGroups.get(1).getLabel(), equalTo("person"));
        assertThat(entityGroups.get(1).getWord(), equalTo("NextPerson NextPersonSecondName"));
        assertThat(entityGroups.get(2).getLabel(), equalTo("organisation"));
    }

    private static NerProcessor.NerResultProcessor createProcessor(List<String> vocab, String input){
        BertTokenizer tokenizer = BertTokenizer.builder(vocab)
            .setDoLowerCase(true)
            .setWithSpecialTokens(false)
            .build();
        BertTokenizer.TokenizationResult tokenizationResult = tokenizer.tokenize(input);
        return new NerProcessor.NerResultProcessor(tokenizationResult, NerProcessor.IobTag.values());
    }

    private static NerProcessor.NerResultProcessor createProcessor(List<String> vocab, String input, NerProcessor.IobTag[] iobMap){
        BertTokenizer tokenizer = BertTokenizer.builder(vocab)
            .setDoLowerCase(true)
            .setWithSpecialTokens(false)
            .build();
        BertTokenizer.TokenizationResult tokenizationResult = tokenizer.tokenize(input);
        return new NerProcessor.NerResultProcessor(tokenizationResult, iobMap);
    }
}
