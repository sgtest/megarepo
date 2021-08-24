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
import java.util.Objects;

public class FillMaskConfig implements NlpConfig {

    public static final String NAME = "fill_mask";

    public static FillMaskConfig fromXContentStrict(XContentParser parser) {
        return STRICT_PARSER.apply(parser, null);
    }

    public static FillMaskConfig fromXContentLenient(XContentParser parser) {
        return LENIENT_PARSER.apply(parser, null);
    }

    private static final ConstructingObjectParser<FillMaskConfig, Void> STRICT_PARSER = createParser(false);
    private static final ConstructingObjectParser<FillMaskConfig, Void> LENIENT_PARSER = createParser(true);

    private static ConstructingObjectParser<FillMaskConfig, Void> createParser(boolean ignoreUnknownFields) {
        ConstructingObjectParser<FillMaskConfig, Void> parser = new ConstructingObjectParser<>(NAME, ignoreUnknownFields,
            a -> new FillMaskConfig((VocabularyConfig) a[0], (Tokenization) a[1]));
        parser.declareObject(ConstructingObjectParser.constructorArg(), VocabularyConfig.createParser(ignoreUnknownFields), VOCABULARY);
        parser.declareNamedObject(
            ConstructingObjectParser.optionalConstructorArg(), (p, c, n) -> p.namedObject(Tokenization.class, n, ignoreUnknownFields),
                TOKENIZATION
        );
        return parser;
    }

    private final VocabularyConfig vocabularyConfig;
    private final Tokenization tokenization;

    public FillMaskConfig(VocabularyConfig vocabularyConfig, @Nullable Tokenization tokenization) {
        this.vocabularyConfig = ExceptionsHelper.requireNonNull(vocabularyConfig, VOCABULARY);
        this.tokenization = tokenization == null ? Tokenization.createDefault() : tokenization;
    }

    public FillMaskConfig(StreamInput in) throws IOException {
        vocabularyConfig = new VocabularyConfig(in);
        tokenization = in.readNamedWriteable(Tokenization.class);
    }

    @Override
    public XContentBuilder toXContent(XContentBuilder builder, Params params) throws IOException {
        builder.startObject();
        builder.field(VOCABULARY.getPreferredName(), vocabularyConfig);
        NamedXContentObjectHelper.writeNamedObject(builder, params, TOKENIZATION.getPreferredName(), tokenization);
        builder.endObject();
        return builder;
    }

    @Override
    public String getWriteableName() {
        return NAME;
    }

    @Override
    public void writeTo(StreamOutput out) throws IOException {
        vocabularyConfig.writeTo(out);
        out.writeNamedWriteable(tokenization);
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

        FillMaskConfig that = (FillMaskConfig) o;
        return Objects.equals(vocabularyConfig, that.vocabularyConfig)
            && Objects.equals(tokenization, that.tokenization);
    }

    @Override
    public int hashCode() {
        return Objects.hash(vocabularyConfig, tokenization);
    }

    @Override
    public VocabularyConfig getVocabularyConfig() {
        return vocabularyConfig;
    }

    @Override
    public Tokenization getTokenization() {
        return tokenization;
    }

    @Override
    public boolean isAllocateOnly() {
        return true;
    }
}
