/*
 * Licensed to Elasticsearch under one or more contributor
 * license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright
 * ownership. Elasticsearch licenses this file to you under
 * the Apache License, Version 2.0 (the "License"); you may
 * not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing,
 * software distributed under the License is distributed on an
 * "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
 * KIND, either express or implied.  See the License for the
 * specific language governing permissions and limitations
 * under the License.
 */
package org.elasticsearch.repositories.s3;

import org.elasticsearch.test.fixture.AbstractHttpFixture;
import com.amazonaws.util.DateUtils;
import org.elasticsearch.common.Strings;
import org.elasticsearch.common.io.Streams;
import org.elasticsearch.common.path.PathTrie;
import org.elasticsearch.common.util.concurrent.ConcurrentCollections;
import org.elasticsearch.rest.RestStatus;
import org.elasticsearch.rest.RestUtils;

import java.io.BufferedInputStream;
import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.util.ArrayList;
import java.util.Date;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

import static java.nio.charset.StandardCharsets.UTF_8;

/**
 * {@link AmazonS3Fixture} emulates an AWS S3 service
 * .
 * he implementation is based on official documentation available at https://docs.aws.amazon.com/AmazonS3/latest/API/.
 */
public class AmazonS3Fixture extends AbstractHttpFixture {

    /** List of the buckets stored on this test server **/
    private final Map<String, Bucket> buckets = ConcurrentCollections.newConcurrentMap();

    /** Request handlers for the requests made by the S3 client **/
    private final PathTrie<RequestHandler> handlers;

    /**
     * Creates a {@link AmazonS3Fixture}
     */
    private AmazonS3Fixture(final String workingDir, final String bucket) {
        super(workingDir);
        this.buckets.put(bucket, new Bucket(bucket));
        this.handlers = defaultHandlers(buckets);
    }

    @Override
    protected Response handle(final Request request) throws IOException {
        final RequestHandler handler = handlers.retrieve(request.getMethod() + " " + request.getPath(), request.getParameters());
        if (handler != null) {
            final String authorization = request.getHeader("Authorization");
            if (authorization == null
                || (authorization.length() > 0 && authorization.contains("s3_integration_test_access_key") == false)) {
                return newError(request.getId(), RestStatus.FORBIDDEN, "AccessDenied", "Access Denied", "");
            }
            return handler.handle(request);
        }
        return null;
    }

    public static void main(final String[] args) throws Exception {
        if (args == null || args.length != 2) {
            throw new IllegalArgumentException("AmazonS3Fixture <working directory> <bucket>");
        }

        final AmazonS3Fixture fixture = new AmazonS3Fixture(args[0], args[1]);
        fixture.listen();
    }

    /** Builds the default request handlers **/
    private static PathTrie<RequestHandler> defaultHandlers(final Map<String, Bucket> buckets) {
        final PathTrie<RequestHandler> handlers = new PathTrie<>(RestUtils.REST_DECODER);

        // HEAD Object
        //
        // https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectHEAD.html
        objectsPaths("HEAD /{bucket}").forEach(path ->
            handlers.insert(path, (request) -> {
                final String bucketName = request.getParam("bucket");

                final Bucket bucket = buckets.get(bucketName);
                if (bucket == null) {
                    return newBucketNotFoundError(request.getId(), bucketName);
                }

                final String objectName = objectName(request.getParameters());
                for (Map.Entry<String, byte[]> object : bucket.objects.entrySet()) {
                    if (object.getKey().equals(objectName)) {
                        return new Response(RestStatus.OK.getStatus(), TEXT_PLAIN_CONTENT_TYPE, EMPTY_BYTE);
                    }
                }
                return newObjectNotFoundError(request.getId(), objectName);
            })
        );

        // PUT Object
        //
        // https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html
        objectsPaths("PUT /{bucket}").forEach(path ->
            handlers.insert(path, (request) -> {
                final String destBucketName = request.getParam("bucket");

                final Bucket destBucket = buckets.get(destBucketName);
                if (destBucket == null) {
                    return newBucketNotFoundError(request.getId(), destBucketName);
                }

                final String destObjectName = objectName(request.getParameters());

                // This is a chunked upload request. We should have the header "Content-Encoding : aws-chunked,gzip"
                // to detect it but it seems that the AWS SDK does not follow the S3 guidelines here.
                //
                // See https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-streaming.html
                //
                String headerDecodedContentLength = request.getHeader("X-amz-decoded-content-length");
                if (headerDecodedContentLength != null) {
                    int contentLength = Integer.valueOf(headerDecodedContentLength);

                    // Chunked requests have a payload like this:
                    //
                    // 105;chunk-signature=01d0de6be013115a7f4794db8c4b9414e6ec71262cc33ae562a71f2eaed1efe8
                    // ...  bytes of data ....
                    // 0;chunk-signature=f890420b1974c5469aaf2112e9e6f2e0334929fd45909e03c0eff7a84124f6a4
                    //
                    try (BufferedInputStream inputStream = new BufferedInputStream(new ByteArrayInputStream(request.getBody()))) {
                        int b;
                        // Moves to the end of the first signature line
                        while ((b = inputStream.read()) != -1) {
                            if (b == '\n') {
                                break;
                            }
                        }

                        final byte[] bytes = new byte[contentLength];
                        inputStream.read(bytes, 0, contentLength);

                        destBucket.objects.put(destObjectName, bytes);
                        return new Response(RestStatus.OK.getStatus(), TEXT_PLAIN_CONTENT_TYPE, EMPTY_BYTE);
                    }
                }

                return newInternalError(request.getId(), "Something is wrong with this PUT request");
            })
        );

        // DELETE Object
        //
        // https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectDELETE.html
        objectsPaths("DELETE /{bucket}").forEach(path ->
            handlers.insert(path, (request) -> {
                final String bucketName = request.getParam("bucket");

                final Bucket bucket = buckets.get(bucketName);
                if (bucket == null) {
                    return newBucketNotFoundError(request.getId(), bucketName);
                }

                final String objectName = objectName(request.getParameters());
                if (bucket.objects.remove(objectName) != null) {
                    return new Response(RestStatus.OK.getStatus(), TEXT_PLAIN_CONTENT_TYPE, EMPTY_BYTE);
                }
                return newObjectNotFoundError(request.getId(), objectName);
            })
        );

        // GET Object
        //
        // https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectGET.html
        objectsPaths("GET /{bucket}").forEach(path ->
            handlers.insert(path, (request) -> {
                final String bucketName = request.getParam("bucket");

                final Bucket bucket = buckets.get(bucketName);
                if (bucket == null) {
                    return newBucketNotFoundError(request.getId(), bucketName);
                }

                final String objectName = objectName(request.getParameters());
                if (bucket.objects.containsKey(objectName)) {
                    return new Response(RestStatus.OK.getStatus(), contentType("application/octet-stream"), bucket.objects.get(objectName));

                }
                return newObjectNotFoundError(request.getId(), objectName);
            })
        );

        // HEAD Bucket
        //
        // https://docs.aws.amazon.com/AmazonS3/latest/API/RESTBucketHEAD.html
        handlers.insert("HEAD /{bucket}", (request) -> {
            String bucket = request.getParam("bucket");
            if (Strings.hasText(bucket) && buckets.containsKey(bucket)) {
                return new Response(RestStatus.OK.getStatus(), TEXT_PLAIN_CONTENT_TYPE, EMPTY_BYTE);
            } else {
                return newBucketNotFoundError(request.getId(), bucket);
            }
        });

        // GET Bucket (List Objects) Version 1
        //
        // https://docs.aws.amazon.com/AmazonS3/latest/API/RESTBucketGET.html
        handlers.insert("GET /{bucket}/", (request) -> {
            final String bucketName = request.getParam("bucket");

            final Bucket bucket = buckets.get(bucketName);
            if (bucket == null) {
                return newBucketNotFoundError(request.getId(), bucketName);
            }

            String prefix = request.getParam("prefix");
            if (prefix == null) {
                prefix = request.getHeader("Prefix");
            }
            return newListBucketResultResponse(request.getId(), bucket, prefix);
        });

        // Delete Multiple Objects
        //
        // https://docs.aws.amazon.com/AmazonS3/latest/API/multiobjectdeleteapi.html
        handlers.insert("POST /", (request) -> {
            final List<String> deletes = new ArrayList<>();
            final List<String> errors = new ArrayList<>();

            if (request.getParam("delete") != null) {
                // The request body is something like:
                // <Delete><Object><Key>...</Key></Object><Object><Key>...</Key></Object></Delete>
                String requestBody = Streams.copyToString(new InputStreamReader(new ByteArrayInputStream(request.getBody()), UTF_8));
                if (requestBody.startsWith("<Delete>")) {
                    final String startMarker = "<Key>";
                    final String endMarker = "</Key>";

                    int offset = 0;
                    while (offset != -1) {
                        offset = requestBody.indexOf(startMarker, offset);
                        if (offset > 0) {
                            int closingOffset = requestBody.indexOf(endMarker, offset);
                            if (closingOffset != -1) {
                                offset = offset + startMarker.length();
                                final String objectName = requestBody.substring(offset, closingOffset);

                                boolean found = false;
                                for (Bucket bucket : buckets.values()) {
                                    if (bucket.objects.remove(objectName) != null) {
                                        found = true;
                                    }
                                }

                                if (found) {
                                    deletes.add(objectName);
                                } else {
                                    errors.add(objectName);
                                }
                            }
                        }
                    }
                    return newDeleteResultResponse(request.getId(), deletes, errors);
                }
            }
            return newInternalError(request.getId(), "Something is wrong with this POST multiple deletes request");
        });

        return handlers;
    }

    /**
     * Represents a S3 bucket.
     */
    static class Bucket {

        /** Bucket name **/
        final String name;

        /** Blobs contained in the bucket **/
        final Map<String, byte[]> objects;

        Bucket(final String name) {
            this.name = Objects.requireNonNull(name);
            this.objects = ConcurrentCollections.newConcurrentMap();
        }
    }

    /**
     * Decline a path like "http://host:port/{bucket}" into 10 derived paths like:
     * - http://host:port/{bucket}/{path0}
     * - http://host:port/{bucket}/{path0}/{path1}
     * - http://host:port/{bucket}/{path0}/{path1}/{path2}
     * - etc
     */
    private static List<String> objectsPaths(final String path) {
        final List<String> paths = new ArrayList<>();
        String p = path;
        for (int i = 0; i < 10; i++) {
            p = p + "/{path" + i + "}";
            paths.add(p);
        }
        return paths;
    }

    /**
     * Retrieves the object name from all derives paths named {pathX} where 0 <= X < 10.
     *
     * This is the counterpart of {@link #objectsPaths(String)}
     */
    private static String objectName(final Map<String, String> params) {
        final StringBuilder name = new StringBuilder();
        for (int i = 0; i < 10; i++) {
            String value = params.getOrDefault("path" + i, null);
            if (value != null) {
                if (name.length() > 0) {
                    name.append('/');
                }
                name.append(value);
            }
        }
        return name.toString();
    }

    /**
     * S3 ListBucketResult Response
     */
    private static Response newListBucketResultResponse(final long requestId, final Bucket bucket, final String prefix) {
        final String id = Long.toString(requestId);
        final StringBuilder response = new StringBuilder();
        response.append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        response.append("<ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">");
        response.append("<Prefix>");
        if (prefix != null) {
            response.append(prefix);
        }
        response.append("</Prefix>");
        response.append("<Marker/>");
        response.append("<MaxKeys>1000</MaxKeys>");
        response.append("<IsTruncated>false</IsTruncated>");

        int count = 0;
        for (Map.Entry<String, byte[]> object : bucket.objects.entrySet()) {
            String objectName = object.getKey();
            if (prefix == null || objectName.startsWith(prefix)) {
                response.append("<Contents>");
                response.append("<Key>").append(objectName).append("</Key>");
                response.append("<LastModified>").append(DateUtils.formatISO8601Date(new Date())).append("</LastModified>");
                response.append("<ETag>&quot;").append(count++).append("&quot;</ETag>");
                response.append("<Size>").append(object.getValue().length).append("</Size>");
                response.append("</Contents>");
            }
        }
        response.append("</ListBucketResult>");

        final Map<String, String> headers = new HashMap<>(contentType("application/xml"));
        headers.put("x-amz-request-id", id);

        return new Response(RestStatus.OK.getStatus(), headers, response.toString().getBytes(UTF_8));
    }

    /**
     * S3 DeleteResult Response
     */
    private static Response newDeleteResultResponse(final long requestId,
                                                    final List<String> deletedObjects,
                                                    final List<String> ignoredObjects) {
        final String id = Long.toString(requestId);

        final StringBuilder response = new StringBuilder();
        response.append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        response.append("<DeleteResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">");
        for (String deletedObject : deletedObjects) {
            response.append("<Deleted>");
            response.append("<Key>").append(deletedObject).append("</Key>");
            response.append("</Deleted>");
        }
        for (String ignoredObject : ignoredObjects) {
            response.append("<Error>");
            response.append("<Key>").append(ignoredObject).append("</Key>");
            response.append("<Code>NoSuchKey</Code>");
            response.append("</Error>");
        }
        response.append("</DeleteResult>");

        final Map<String, String> headers = new HashMap<>(contentType("application/xml"));
        headers.put("x-amz-request-id", id);

        return new Response(RestStatus.OK.getStatus(), headers, response.toString().getBytes(UTF_8));
    }

    private static Response newBucketNotFoundError(final long requestId, final String bucket) {
        return newError(requestId, RestStatus.NOT_FOUND, "NoSuchBucket", "The specified bucket does not exist", bucket);
    }

    private static Response newObjectNotFoundError(final long requestId, final String object) {
        return newError(requestId, RestStatus.NOT_FOUND, "NoSuchKey", "The specified key does not exist", object);
    }

    private static Response newInternalError(final long requestId, final String resource) {
        return newError(requestId, RestStatus.INTERNAL_SERVER_ERROR, "InternalError", "We encountered an internal error", resource);
    }

    /**
     * S3 Error
     *
     * https://docs.aws.amazon.com/AmazonS3/latest/API/ErrorResponses.html
     */
    private static Response newError(final long requestId,
                                     final RestStatus status,
                                     final String code,
                                     final String message,
                                     final String resource) {
        final String id = Long.toString(requestId);
        final StringBuilder response = new StringBuilder();
        response.append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        response.append("<Error>");
        response.append("<Code>").append(code).append("</Code>");
        response.append("<Message>").append(message).append("</Message>");
        response.append("<Resource>").append(resource).append("</Resource>");
        response.append("<RequestId>").append(id).append("</RequestId>");
        response.append("</Error>");

        final Map<String, String> headers = new HashMap<>(contentType("application/xml"));
        headers.put("x-amz-request-id", id);

        return new Response(status.getStatus(), headers, response.toString().getBytes(UTF_8));
    }
}
