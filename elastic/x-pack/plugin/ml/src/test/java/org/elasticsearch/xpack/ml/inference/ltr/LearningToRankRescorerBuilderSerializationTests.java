/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.ml.inference.ltr;

import org.elasticsearch.TransportVersion;
import org.elasticsearch.common.ParsingException;
import org.elasticsearch.common.io.stream.NamedWriteableRegistry;
import org.elasticsearch.common.io.stream.Writeable;
import org.elasticsearch.common.settings.Settings;
import org.elasticsearch.core.Tuple;
import org.elasticsearch.search.SearchModule;
import org.elasticsearch.xcontent.NamedXContentRegistry;
import org.elasticsearch.xcontent.XContentParser;
import org.elasticsearch.xcontent.XContentType;
import org.elasticsearch.xpack.core.ml.AbstractBWCSerializationTestCase;
import org.elasticsearch.xpack.core.ml.inference.MlInferenceNamedXContentProvider;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.LearningToRankConfig;
import org.elasticsearch.xpack.core.ml.ltr.MlLTRNamedXContentProvider;
import org.elasticsearch.xpack.ml.inference.loadingservice.LocalModel;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;

import static org.elasticsearch.search.rank.RankBuilder.WINDOW_SIZE_FIELD;
import static org.elasticsearch.xpack.core.ml.inference.trainedmodel.LearningToRankConfigTests.randomLearningToRankConfig;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

public class LearningToRankRescorerBuilderSerializationTests extends AbstractBWCSerializationTestCase<LearningToRankRescorerBuilder> {

    private static LearningToRankService learningToRankService = mock(LearningToRankService.class);

    @Override
    protected LearningToRankRescorerBuilder doParseInstance(XContentParser parser) throws IOException {
        String fieldName = null;
        LearningToRankRescorerBuilder rescorer = null;
        Integer windowSize = null;
        XContentParser.Token token = parser.nextToken();
        assert token == XContentParser.Token.START_OBJECT;
        while ((token = parser.nextToken()) != XContentParser.Token.END_OBJECT) {
            if (token == XContentParser.Token.FIELD_NAME) {
                fieldName = parser.currentName();
            } else if (token.isValue()) {
                if (WINDOW_SIZE_FIELD.match(fieldName, parser.getDeprecationHandler())) {
                    windowSize = parser.intValue();
                } else {
                    throw new ParsingException(parser.getTokenLocation(), "rescore doesn't support [" + fieldName + "]");
                }
            } else if (token == XContentParser.Token.START_OBJECT) {
                rescorer = LearningToRankRescorerBuilder.fromXContent(parser, learningToRankService);
            } else {
                throw new ParsingException(parser.getTokenLocation(), "unexpected token [" + token + "] after [" + fieldName + "]");
            }
        }
        if (rescorer == null) {
            throw new ParsingException(parser.getTokenLocation(), "missing rescore type");
        }
        if (windowSize != null) {
            rescorer.windowSize(windowSize);
        }
        return rescorer;
    }

    @Override
    protected Writeable.Reader<LearningToRankRescorerBuilder> instanceReader() {
        return in -> new LearningToRankRescorerBuilder(in, learningToRankService);
    }

    @Override
    protected LearningToRankRescorerBuilder createTestInstance() {
        LearningToRankRescorerBuilder builder = randomBoolean()
            ? createXContextTestInstance(null)
            : new LearningToRankRescorerBuilder(
                randomAlphaOfLength(10),
                randomLearningToRankConfig(),
                randomBoolean() ? randomParams() : null,
                learningToRankService
            );

        if (randomBoolean()) {
            builder.windowSize(randomIntBetween(1, 10000));
        }

        return builder;
    }

    @Override
    protected LearningToRankRescorerBuilder createXContextTestInstance(XContentType xContentType) {
        return new LearningToRankRescorerBuilder(randomAlphaOfLength(10), randomBoolean() ? randomParams() : null, learningToRankService);
    }

    @Override
    protected LearningToRankRescorerBuilder mutateInstance(LearningToRankRescorerBuilder instance) throws IOException {

        int i = randomInt(4);
        return switch (i) {
            case 0 -> {
                LearningToRankRescorerBuilder builder = new LearningToRankRescorerBuilder(
                    randomValueOtherThan(instance.modelId(), () -> randomAlphaOfLength(10)),
                    instance.params(),
                    learningToRankService
                );
                if (instance.windowSize() != null) {
                    builder.windowSize(instance.windowSize());
                }
                yield builder;
            }
            case 1 -> new LearningToRankRescorerBuilder(instance.modelId(), instance.params(), learningToRankService).windowSize(
                randomValueOtherThan(instance.windowSize(), () -> randomIntBetween(1, 10000))
            );
            case 2 -> {
                LearningToRankRescorerBuilder builder = new LearningToRankRescorerBuilder(
                    instance.modelId(),
                    randomValueOtherThan(instance.params(), () -> (randomBoolean() ? randomParams() : null)),
                    learningToRankService
                );
                if (instance.windowSize() != null) {
                    builder.windowSize(instance.windowSize() + 1);
                }
                yield builder;
            }
            case 3 -> {
                LearningToRankConfig learningToRankConfig = randomValueOtherThan(
                    instance.learningToRankConfig(),
                    () -> randomLearningToRankConfig()
                );
                LearningToRankRescorerBuilder builder = new LearningToRankRescorerBuilder(
                    instance.modelId(),
                    learningToRankConfig,
                    null,
                    learningToRankService
                );
                if (instance.windowSize() != null) {
                    builder.windowSize(instance.windowSize());
                }
                yield builder;
            }
            case 4 -> {
                LearningToRankRescorerBuilder builder = new LearningToRankRescorerBuilder(
                    mock(LocalModel.class),
                    instance.learningToRankConfig(),
                    instance.params(),
                    learningToRankService
                );
                if (instance.windowSize() != null) {
                    builder.windowSize(instance.windowSize());
                }
                yield builder;
            }
            default -> throw new AssertionError("Unexpected random test case");
        };
    }

    @Override
    protected LearningToRankRescorerBuilder mutateInstanceForVersion(LearningToRankRescorerBuilder instance, TransportVersion version) {
        return instance;
    }

    @Override
    protected NamedXContentRegistry xContentRegistry() {
        List<NamedXContentRegistry.Entry> namedXContent = new ArrayList<>();
        namedXContent.addAll(new MlInferenceNamedXContentProvider().getNamedXContentParsers());
        namedXContent.addAll(new MlLTRNamedXContentProvider().getNamedXContentParsers());
        namedXContent.addAll(new SearchModule(Settings.EMPTY, Collections.emptyList()).getNamedXContents());
        return new NamedXContentRegistry(namedXContent);
    }

    @Override
    protected NamedWriteableRegistry writableRegistry() {
        List<NamedWriteableRegistry.Entry> namedWriteables = new ArrayList<>(new MlInferenceNamedXContentProvider().getNamedWriteables());
        namedWriteables.addAll(new MlLTRNamedXContentProvider().getNamedWriteables());
        namedWriteables.addAll(new SearchModule(Settings.EMPTY, Collections.emptyList()).getNamedWriteables());
        return new NamedWriteableRegistry(namedWriteables);
    }

    @Override
    protected NamedWriteableRegistry getNamedWriteableRegistry() {
        return writableRegistry();
    }

    private static Map<String, Object> randomParams() {
        return randomMap(1, randomIntBetween(1, 10), () -> new Tuple<>(randomIdentifier(), randomIdentifier()));
    }

    private static LocalModel localModelMock() {
        LocalModel model = mock(LocalModel.class);
        String modelId = randomIdentifier();
        when(model.getModelId()).thenReturn(modelId);
        return model;
    }
}
