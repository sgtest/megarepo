/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.ml.inference.trainedmodel;

import org.elasticsearch.Version;
import org.elasticsearch.common.io.stream.StreamInput;
import org.elasticsearch.common.io.stream.StreamOutput;
import org.elasticsearch.common.xcontent.ConstructingObjectParser;
import org.elasticsearch.common.xcontent.XContentBuilder;
import org.elasticsearch.common.xcontent.XContentParser;
import org.elasticsearch.core.Nullable;
import org.elasticsearch.xpack.core.ml.utils.ExceptionsHelper;
import org.elasticsearch.xpack.core.ml.utils.NamedXContentObjectHelper;

import java.io.IOException;
import java.util.Collections;
import java.util.List;
import java.util.Objects;

public class SentimentAnalysisConfig implements NlpConfig {

    public static final String NAME = "sentiment_analysis";

    public static SentimentAnalysisConfig fromXContentStrict(XContentParser parser) {
        return STRICT_PARSER.apply(parser, null);
    }

    public static SentimentAnalysisConfig fromXContentLenient(XContentParser parser) {
        return LENIENT_PARSER.apply(parser, null);
    }

    private static final ConstructingObjectParser<SentimentAnalysisConfig, Void> STRICT_PARSER = createParser(false);
    private static final ConstructingObjectParser<SentimentAnalysisConfig, Void> LENIENT_PARSER = createParser(true);

    @SuppressWarnings({ "unchecked"})
    private static ConstructingObjectParser<SentimentAnalysisConfig, Void> createParser(boolean ignoreUnknownFields) {
        ConstructingObjectParser<SentimentAnalysisConfig, Void> parser = new ConstructingObjectParser<>(NAME, ignoreUnknownFields,
            a -> new SentimentAnalysisConfig((VocabularyConfig) a[0], (Tokenization) a[1], (List<String>) a[2]));
        parser.declareObject(ConstructingObjectParser.constructorArg(), VocabularyConfig.createParser(ignoreUnknownFields), VOCABULARY);
        parser.declareNamedObject(
            ConstructingObjectParser.optionalConstructorArg(), (p, c, n) -> p.namedObject(Tokenization.class, n, ignoreUnknownFields),
                TOKENIZATION
        );
        parser.declareStringArray(ConstructingObjectParser.optionalConstructorArg(), CLASSIFICATION_LABELS);
        return parser;
    }

    private final VocabularyConfig vocabularyConfig;
    private final Tokenization tokenization;
    private final List<String> classificationLabels;

    public SentimentAnalysisConfig(VocabularyConfig vocabularyConfig, @Nullable Tokenization tokenization,
                                   @Nullable List<String> classificationLabels) {
        this.vocabularyConfig = ExceptionsHelper.requireNonNull(vocabularyConfig, VOCABULARY);
        this.tokenization = tokenization == null ? Tokenization.createDefault() : tokenization;
        this.classificationLabels = classificationLabels == null ? Collections.emptyList() : classificationLabels;
    }

    public SentimentAnalysisConfig(StreamInput in) throws IOException {
        vocabularyConfig = new VocabularyConfig(in);
        tokenization = in.readNamedWriteable(Tokenization.class);
        classificationLabels = in.readStringList();
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        vocabularyConfig.writeTo(out);
        out.writeNamedWriteable(tokenization);
        out.writeStringCollection(classificationLabels);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(VOCABULARY.getPreferredName(), vocabularyConfig);
        NamedXContentObjectHelper.writeNamedObject(builder, params, TOKENIZATION.getPreferredName(), tokenization);
        if (classificationLabels.isEmpty() == false) {
            builder.field(CLASSIFICATION_LABELS.getPreferredName(), classificationLabels);
        }
        builder.endObject();
        return builder;
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    @Override
    public boolean isTargetTypeSupported(TargetType targetType) {
        return false;
    }

    @Override
    public Version getMinimalSupportedVersion() {
        return Version.V_8_0_0;
    }

    @Override
    public String getName() {
        return NAME;
    }

    @Override
    public boolean equals(Object o) {
        if (o == this) return true;
        if (o == null || getClass() != o.getClass()) return false;

        SentimentAnalysisConfig that = (SentimentAnalysisConfig) o;
        return Objects.equals(vocabularyConfig, that.vocabularyConfig)
            && Objects.equals(tokenization, that.tokenization)
            && Objects.equals(classificationLabels, that.classificationLabels);
    }

    @Override
    public int hashCode() {
        return Objects.hash(vocabularyConfig, tokenization, classificationLabels);
    }

    @Override
    public VocabularyConfig getVocabularyConfig() {
        return vocabularyConfig;
    }

    @Override
    public Tokenization getTokenization() {
        return tokenization;
    }

    public List<String> getClassificationLabels() {
        return classificationLabels;
    }

    @Override
    public boolean isAllocateOnly() {
        return true;
    }
}
