/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License
 * 2.0; you may not use this file except in compliance with the Elastic License
 * 2.0.
 */

package org.elasticsearch.xpack.core.ml.inference;

import org.elasticsearch.xpack.core.ml.inference.trainedmodel.TrainedModel;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.ensemble.Ensemble;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.langident.LangIdentNeuralNetwork;
import org.elasticsearch.xpack.core.ml.inference.trainedmodel.tree.Tree;

import java.util.Locale;

public enum TrainedModelType {

    TREE_ENSEMBLE,
    LANG_IDENT,
    PYTORCH {
        @Override
        public boolean hasInferenceDefinition() {
            return false;
        }
    };

    public static TrainedModelType fromString(String name) {
        return valueOf(name.trim().toUpperCase(Locale.ROOT));
    }

    /**
     * Introspect the given model and return the model type
     * representing it.
     * @param model A Trained model
     * @return The model type or null if unknown
     */
    public static TrainedModelType typeFromTrainedModel(TrainedModel model) {
        if (model instanceof Ensemble || model instanceof Tree) {
            return TrainedModelType.TREE_ENSEMBLE;
        } else if (model instanceof LangIdentNeuralNetwork) {
            return TrainedModelType.LANG_IDENT;
        } else {
            return null;
        }
    }

    public boolean hasInferenceDefinition() {
        return true;
    }

    @Override
    public String toString() {
        return name().toLowerCase(Locale.ROOT);
    }
}
