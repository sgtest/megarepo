/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.core.dataframe;

import java.text.MessageFormat;
import java.util.Locale;

public class DataFrameMessages {

    public static final String REST_STOP_TRANSFORM_WAIT_FOR_COMPLETION_TIMEOUT =
            "Timed out after [{0}] while waiting for data frame transform [{1}] to stop";
    public static final String REST_STOP_TRANSFORM_WAIT_FOR_COMPLETION_INTERRUPT =
            "Interrupted while waiting for data frame transform [{0}] to stop";
    public static final String REST_PUT_DATA_FRAME_TRANSFORM_EXISTS = "Transform with id [{0}] already exists";
    public static final String REST_DATA_FRAME_UNKNOWN_TRANSFORM = "Transform with id [{0}] could not be found";
    public static final String REST_PUT_DATA_FRAME_FAILED_TO_VALIDATE_DATA_FRAME_CONFIGURATION =
            "Failed to validate data frame configuration";
    public static final String REST_PUT_DATA_FRAME_FAILED_PERSIST_TRANSFORM_CONFIGURATION = "Failed to persist data frame configuration";
    public static final String REST_PUT_DATA_FRAME_FAILED_TO_DEDUCE_TARGET_MAPPINGS = "Failed to deduce target mappings";
    public static final String REST_PUT_DATA_FRAME_FAILED_TO_CREATE_TARGET_INDEX = "Failed to create target index";
    public static final String REST_PUT_DATA_FRAME_FAILED_TO_START_PERSISTENT_TASK =
            "Failed to start persistent task, configuration has been cleaned up: [{0}]";
    public static final String REST_DATA_FRAME_FAILED_TO_SERIALIZE_TRANSFORM = "Failed to serialise transform [{0}]";

    public static final String FAILED_TO_CREATE_DESTINATION_INDEX = "Could not create destination index [{0}] for transform[{1}]";
    public static final String FAILED_TO_LOAD_TRANSFORM_CONFIGURATION =
            "Failed to load data frame transform configuration for transform [{0}]";
    public static final String FAILED_TO_PARSE_TRANSFORM_CONFIGURATION =
            "Failed to parse transform configuration for data frame transform [{0}]";
    public static final String DATA_FRAME_TRANSFORM_CONFIGURATION_NO_TRANSFORM =
            "Data frame transform configuration must specify exactly 1 function";
    public static final String DATA_FRAME_TRANSFORM_CONFIGURATION_PIVOT_NO_GROUP_BY =
            "Data frame pivot transform configuration must specify at least 1 group_by";
    public static final String DATA_FRAME_TRANSFORM_CONFIGURATION_PIVOT_NO_AGGREGATION =
            "Data frame pivot transform configuration must specify at least 1 aggregation";
    public static final String DATA_FRAME_TRANSFORM_PIVOT_FAILED_TO_CREATE_COMPOSITE_AGGREGATION =
            "Failed to create composite aggregation from pivot function";
    public static final String DATA_FRAME_TRANSFORM_CONFIGURATION_INVALID =
            "Data frame transform configuration [{0}] has invalid elements";

    public static final String LOG_DATA_FRAME_TRANSFORM_CONFIGURATION_BAD_QUERY =
            "Failed to parse query for data frame transform";
    public static final String LOG_DATA_FRAME_TRANSFORM_CONFIGURATION_BAD_GROUP_BY =
            "Failed to parse group_by for data frame pivot transform";
    public static final String LOG_DATA_FRAME_TRANSFORM_CONFIGURATION_BAD_AGGREGATION =
            "Failed to parse aggregation for data frame pivot transform";

    private DataFrameMessages() {
    }

    /**
     * Returns the message parameter
     *
     * @param message Should be one of the statics defined in this class
     */
    public static String getMessage(String message) {
        return message;
    }

    /**
     * Format the message with the supplied arguments
     *
     * @param message Should be one of the statics defined in this class
     * @param args MessageFormat arguments. See {@linkplain MessageFormat#format(Object)}]
     */
    public static String getMessage(String message, Object... args) {
        return new MessageFormat(message, Locale.ROOT).format(args);
    }
}
