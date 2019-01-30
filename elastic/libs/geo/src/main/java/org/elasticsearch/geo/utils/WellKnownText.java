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

package org.elasticsearch.geo.utils;

import org.elasticsearch.geo.geometry.Circle;
import org.elasticsearch.geo.geometry.Geometry;
import org.elasticsearch.geo.geometry.GeometryCollection;
import org.elasticsearch.geo.geometry.GeometryVisitor;
import org.elasticsearch.geo.geometry.Line;
import org.elasticsearch.geo.geometry.LinearRing;
import org.elasticsearch.geo.geometry.MultiLine;
import org.elasticsearch.geo.geometry.MultiPoint;
import org.elasticsearch.geo.geometry.MultiPolygon;
import org.elasticsearch.geo.geometry.Point;
import org.elasticsearch.geo.geometry.Polygon;
import org.elasticsearch.geo.geometry.Rectangle;

import java.io.IOException;
import java.io.StreamTokenizer;
import java.io.StringReader;
import java.text.ParseException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

/**
 * Utility class for converting to and from WKT
 */
public class WellKnownText {
    public static final String EMPTY = "EMPTY";
    public static final String SPACE = " ";
    public static final String LPAREN = "(";
    public static final String RPAREN = ")";
    public static final String COMMA = ",";
    public static final String NAN = "NaN";

    private static final String NUMBER = "<NUMBER>";
    private static final String EOF = "END-OF-STREAM";
    private static final String EOL = "END-OF-LINE";

    public static String toWKT(Geometry geometry) {
        StringBuilder builder = new StringBuilder();
        toWKT(geometry, builder);
        return builder.toString();
    }

    public static void toWKT(Geometry geometry, StringBuilder sb) {
        sb.append(getWKTName(geometry));
        sb.append(SPACE);
        if (geometry.isEmpty()) {
            sb.append(EMPTY);
        } else {
            geometry.visit(new GeometryVisitor<Void>() {
                @Override
                public Void visit(Circle circle) {
                    sb.append(LPAREN);
                    visitPoint(circle.getLon(), circle.getLat());
                    sb.append(SPACE);
                    sb.append(circle.getRadiusMeters());
                    sb.append(RPAREN);
                    return null;
                }

                @Override
                public Void visit(GeometryCollection<?> collection) {
                    if (collection.size() == 0) {
                        sb.append(EMPTY);
                    } else {
                        sb.append(LPAREN);
                        toWKT(collection.get(0), sb);
                        for (int i = 1; i < collection.size(); ++i) {
                            sb.append(COMMA);
                            toWKT(collection.get(i), sb);
                        }
                        sb.append(RPAREN);
                    }
                    return null;
                }

                @Override
                public Void visit(Line line) {
                    sb.append(LPAREN);
                    visitPoint(line.getLon(0), line.getLat(0));
                    for (int i = 1; i < line.length(); ++i) {
                        sb.append(COMMA);
                        sb.append(SPACE);
                        visitPoint(line.getLon(i), line.getLat(i));
                    }
                    sb.append(RPAREN);
                    return null;
                }

                @Override
                public Void visit(LinearRing ring) {
                    throw new IllegalArgumentException("Linear ring is not supported by WKT");
                }

                @Override
                public Void visit(MultiLine multiLine) {
                    visitCollection(multiLine);
                    return null;
                }

                @Override
                public Void visit(MultiPoint multiPoint) {
                    if (multiPoint.isEmpty()) {
                        sb.append(EMPTY);
                        return null;
                    }
                    // walk through coordinates:
                    sb.append(LPAREN);
                    visitPoint(multiPoint.get(0).getLon(), multiPoint.get(0).getLat());
                    for (int i = 1; i < multiPoint.size(); ++i) {
                        sb.append(COMMA);
                        sb.append(SPACE);
                        Point point = multiPoint.get(i);
                        visitPoint(point.getLon(), point.getLat());
                    }
                    sb.append(RPAREN);
                    return null;
                }

                @Override
                public Void visit(MultiPolygon multiPolygon) {
                    visitCollection(multiPolygon);
                    return null;
                }

                @Override
                public Void visit(Point point) {
                    if (point.isEmpty()) {
                        sb.append(EMPTY);
                    } else {
                        sb.append(LPAREN);
                        visitPoint(point.getLon(), point.getLat());
                        sb.append(RPAREN);
                    }
                    return null;
                }

                private void visitPoint(double lon, double lat) {
                    sb.append(lon).append(SPACE).append(lat);
                }

                private void visitCollection(GeometryCollection<?> collection) {
                    if (collection.size() == 0) {
                        sb.append(EMPTY);
                    } else {
                        sb.append(LPAREN);
                        collection.get(0).visit(this);
                        for (int i = 1; i < collection.size(); ++i) {
                            sb.append(COMMA);
                            collection.get(i).visit(this);
                        }
                        sb.append(RPAREN);
                    }
                }

                @Override
                public Void visit(Polygon polygon) {
                    sb.append(LPAREN);
                    visit((Line) polygon.getPolygon());
                    int numberOfHoles = polygon.getNumberOfHoles();
                    for (int i = 0; i < numberOfHoles; ++i) {
                        sb.append(", ");
                        visit((Line) polygon.getHole(i));
                    }
                    sb.append(RPAREN);
                    return null;
                }

                @Override
                public Void visit(Rectangle rectangle) {
                    sb.append(LPAREN);
                    // minX, maxX, maxY, minY
                    sb.append(rectangle.getMinLon());
                    sb.append(COMMA);
                    sb.append(SPACE);
                    sb.append(rectangle.getMaxLon());
                    sb.append(COMMA);
                    sb.append(SPACE);
                    sb.append(rectangle.getMaxLat());
                    sb.append(COMMA);
                    sb.append(SPACE);
                    sb.append(rectangle.getMinLat());
                    sb.append(RPAREN);
                    return null;
                }
            });
        }
    }

    public static Geometry fromWKT(String wkt) throws IOException, ParseException {
        StringReader reader = new StringReader(wkt);
        try {
            // setup the tokenizer; configured to read words w/o numbers
            StreamTokenizer tokenizer = new StreamTokenizer(reader);
            tokenizer.resetSyntax();
            tokenizer.wordChars('a', 'z');
            tokenizer.wordChars('A', 'Z');
            tokenizer.wordChars(128 + 32, 255);
            tokenizer.wordChars('0', '9');
            tokenizer.wordChars('-', '-');
            tokenizer.wordChars('+', '+');
            tokenizer.wordChars('.', '.');
            tokenizer.whitespaceChars(' ', ' ');
            tokenizer.whitespaceChars('\t', '\t');
            tokenizer.whitespaceChars('\r', '\r');
            tokenizer.whitespaceChars('\n', '\n');
            tokenizer.commentChar('#');
            return parseGeometry(tokenizer);
        } finally {
            reader.close();
        }
    }

    /**
     * parse geometry from the stream tokenizer
     */
    private static Geometry parseGeometry(StreamTokenizer stream) throws IOException, ParseException {
        final String type = nextWord(stream).toLowerCase(Locale.ROOT);
        switch (type) {
            case "point":
                return parsePoint(stream);
            case "multipoint":
                return parseMultiPoint(stream);
            case "linestring":
                return parseLine(stream);
            case "multilinestring":
                return parseMultiLine(stream);
            case "polygon":
                return parsePolygon(stream);
            case "multipolygon":
                return parseMultiPolygon(stream);
            case "bbox":
                return parseBBox(stream);
            case "geometrycollection":
                return parseGeometryCollection(stream);
            case "circle": // Not part of the standard, but we need it for internal serialization
                return parseCircle(stream);
        }
        throw new IllegalArgumentException("Unknown geometry type: " + type);
    }

    private static GeometryCollection<Geometry> parseGeometryCollection(StreamTokenizer stream) throws IOException, ParseException {
        if (nextEmptyOrOpen(stream).equals(EMPTY)) {
            return GeometryCollection.EMPTY;
        }
        List<Geometry> shapes = new ArrayList<>();
        shapes.add(parseGeometry(stream));
        while (nextCloserOrComma(stream).equals(COMMA)) {
            shapes.add(parseGeometry(stream));
        }
        return new GeometryCollection<>(shapes);
    }

    private static Point parsePoint(StreamTokenizer stream) throws IOException, ParseException {
        if (nextEmptyOrOpen(stream).equals(EMPTY)) {
            return Point.EMPTY;
        }
        double lon = nextNumber(stream);
        double lat = nextNumber(stream);
        Point pt = new Point(lat, lon);
        if (isNumberNext(stream) == true) {
            nextNumber(stream);
        }
        nextCloser(stream);
        return pt;
    }

    private static void parseCoordinates(StreamTokenizer stream, ArrayList<Double> lats, ArrayList<Double> lons)
        throws IOException, ParseException {
        parseCoordinate(stream, lats, lons);
        while (nextCloserOrComma(stream).equals(COMMA)) {
            parseCoordinate(stream, lats, lons);
        }
    }

    private static void parseCoordinate(StreamTokenizer stream, ArrayList<Double> lats, ArrayList<Double> lons)
        throws IOException, ParseException {
        lons.add(nextNumber(stream));
        lats.add(nextNumber(stream));
        if (isNumberNext(stream)) {
            nextNumber(stream);
        }
    }

    private static MultiPoint parseMultiPoint(StreamTokenizer stream) throws IOException, ParseException {
        String token = nextEmptyOrOpen(stream);
        if (token.equals(EMPTY)) {
            return MultiPoint.EMPTY;
        }
        ArrayList<Double> lats = new ArrayList<>();
        ArrayList<Double> lons = new ArrayList<>();
        ArrayList<Point> points = new ArrayList<>();
        parseCoordinates(stream, lats, lons);
        for (int i = 0; i < lats.size(); i++) {
            points.add(new Point(lats.get(i), lons.get(i)));
        }
        return new MultiPoint(Collections.unmodifiableList(points));
    }

    private static Line parseLine(StreamTokenizer stream) throws IOException, ParseException {
        String token = nextEmptyOrOpen(stream);
        if (token.equals(EMPTY)) {
            return Line.EMPTY;
        }
        ArrayList<Double> lats = new ArrayList<>();
        ArrayList<Double> lons = new ArrayList<>();
        parseCoordinates(stream, lats, lons);
        return new Line(lats.stream().mapToDouble(i -> i).toArray(), lons.stream().mapToDouble(i -> i).toArray());
    }

    private static MultiLine parseMultiLine(StreamTokenizer stream) throws IOException, ParseException {
        String token = nextEmptyOrOpen(stream);
        if (token.equals(EMPTY)) {
            return MultiLine.EMPTY;
        }
        ArrayList<Line> lines = new ArrayList<>();
        lines.add(parseLine(stream));
        while (nextCloserOrComma(stream).equals(COMMA)) {
            lines.add(parseLine(stream));
        }
        return new MultiLine(Collections.unmodifiableList(lines));
    }

    private static LinearRing parsePolygonHole(StreamTokenizer stream) throws IOException, ParseException {
        nextOpener(stream);
        ArrayList<Double> lats = new ArrayList<>();
        ArrayList<Double> lons = new ArrayList<>();
        parseCoordinates(stream, lats, lons);
        return new LinearRing(lats.stream().mapToDouble(i -> i).toArray(), lons.stream().mapToDouble(i -> i).toArray());
    }

    private static Polygon parsePolygon(StreamTokenizer stream) throws IOException, ParseException {
        if (nextEmptyOrOpen(stream).equals(EMPTY)) {
            return Polygon.EMPTY;
        }
        nextOpener(stream);
        ArrayList<Double> lats = new ArrayList<>();
        ArrayList<Double> lons = new ArrayList<>();
        parseCoordinates(stream, lats, lons);
        ArrayList<LinearRing> holes = new ArrayList<>();
        while (nextCloserOrComma(stream).equals(COMMA)) {
            holes.add(parsePolygonHole(stream));
        }
        if (holes.isEmpty()) {
            return new Polygon(new LinearRing(lats.stream().mapToDouble(i -> i).toArray(), lons.stream().mapToDouble(i -> i).toArray()));
        } else {
            return new Polygon(
                new LinearRing(lats.stream().mapToDouble(i -> i).toArray(), lons.stream().mapToDouble(i -> i).toArray()),
                Collections.unmodifiableList(holes));
        }
    }

    private static MultiPolygon parseMultiPolygon(StreamTokenizer stream) throws IOException, ParseException {
        String token = nextEmptyOrOpen(stream);
        if (token.equals(EMPTY)) {
            return MultiPolygon.EMPTY;
        }
        ArrayList<Polygon> polygons = new ArrayList<>();
        polygons.add(parsePolygon(stream));
        while (nextCloserOrComma(stream).equals(COMMA)) {
            polygons.add(parsePolygon(stream));
        }
        return new MultiPolygon(Collections.unmodifiableList(polygons));
    }

    private static Rectangle parseBBox(StreamTokenizer stream) throws IOException, ParseException {
        if (nextEmptyOrOpen(stream).equals(EMPTY)) {
            return Rectangle.EMPTY;
        }
        double minLon = nextNumber(stream);
        nextComma(stream);
        double maxLon = nextNumber(stream);
        nextComma(stream);
        double maxLat = nextNumber(stream);
        nextComma(stream);
        double minLat = nextNumber(stream);
        nextCloser(stream);
        return new Rectangle(minLat, maxLat, minLon, maxLon);
    }


    private static Circle parseCircle(StreamTokenizer stream) throws IOException, ParseException {
        if (nextEmptyOrOpen(stream).equals(EMPTY)) {
            return Circle.EMPTY;
        }
        double lon = nextNumber(stream);
        double lat = nextNumber(stream);
        double radius = nextNumber(stream);
        Circle circle = new Circle(lat, lon, radius);
        if (isNumberNext(stream) == true) {
            nextNumber(stream);
        }
        nextCloser(stream);
        return circle;
    }

    /**
     * next word in the stream
     */
    private static String nextWord(StreamTokenizer stream) throws ParseException, IOException {
        switch (stream.nextToken()) {
            case StreamTokenizer.TT_WORD:
                final String word = stream.sval;
                return word.equalsIgnoreCase(EMPTY) ? EMPTY : word;
            case '(':
                return LPAREN;
            case ')':
                return RPAREN;
            case ',':
                return COMMA;
        }
        throw new ParseException("expected word but found: " + tokenString(stream), stream.lineno());
    }

    private static double nextNumber(StreamTokenizer stream) throws IOException, ParseException {
        if (stream.nextToken() == StreamTokenizer.TT_WORD) {
            if (stream.sval.equalsIgnoreCase(NAN)) {
                return Double.NaN;
            } else {
                try {
                    return Double.parseDouble(stream.sval);
                } catch (NumberFormatException e) {
                    throw new ParseException("invalid number found: " + stream.sval, stream.lineno());
                }
            }
        }
        throw new ParseException("expected number but found: " + tokenString(stream), stream.lineno());
    }

    private static String tokenString(StreamTokenizer stream) {
        switch (stream.ttype) {
            case StreamTokenizer.TT_WORD:
                return stream.sval;
            case StreamTokenizer.TT_EOF:
                return EOF;
            case StreamTokenizer.TT_EOL:
                return EOL;
            case StreamTokenizer.TT_NUMBER:
                return NUMBER;
        }
        return "'" + (char) stream.ttype + "'";
    }

    private static boolean isNumberNext(StreamTokenizer stream) throws IOException {
        final int type = stream.nextToken();
        stream.pushBack();
        return type == StreamTokenizer.TT_WORD;
    }

    private static String nextEmptyOrOpen(StreamTokenizer stream) throws IOException, ParseException {
        final String next = nextWord(stream);
        if (next.equals(EMPTY) || next.equals(LPAREN)) {
            return next;
        }
        throw new ParseException("expected " + EMPTY + " or " + LPAREN
            + " but found: " + tokenString(stream), stream.lineno());
    }

    private static String nextCloser(StreamTokenizer stream) throws IOException, ParseException {
        if (nextWord(stream).equals(RPAREN)) {
            return RPAREN;
        }
        throw new ParseException("expected " + RPAREN + " but found: " + tokenString(stream), stream.lineno());
    }

    private static String nextComma(StreamTokenizer stream) throws IOException, ParseException {
        if (nextWord(stream).equals(COMMA) == true) {
            return COMMA;
        }
        throw new ParseException("expected " + COMMA + " but found: " + tokenString(stream), stream.lineno());
    }

    private static String nextOpener(StreamTokenizer stream) throws IOException, ParseException {
        if (nextWord(stream).equals(LPAREN)) {
            return LPAREN;
        }
        throw new ParseException("expected " + LPAREN + " but found: " + tokenString(stream), stream.lineno());
    }

    private static String nextCloserOrComma(StreamTokenizer stream) throws IOException, ParseException {
        String token = nextWord(stream);
        if (token.equals(COMMA) || token.equals(RPAREN)) {
            return token;
        }
        throw new ParseException("expected " + COMMA + " or " + RPAREN
            + " but found: " + tokenString(stream), stream.lineno());
    }

    public static String getWKTName(Geometry geometry) {
        return geometry.visit(new GeometryVisitor<String>() {
            @Override
            public String visit(Circle circle) {
                return "circle";
            }

            @Override
            public String visit(GeometryCollection<?> collection) {
                return "geometrycollection";
            }

            @Override
            public String visit(Line line) {
                return "linestring";
            }

            @Override
            public String visit(LinearRing ring) {
                throw new UnsupportedOperationException("line ring cannot be serialized using WKT");
            }

            @Override
            public String visit(MultiLine multiLine) {
                return "multilinestring";
            }

            @Override
            public String visit(MultiPoint multiPoint) {
                return "multipoint";
            }

            @Override
            public String visit(MultiPolygon multiPolygon) {
                return "multipolygon";
            }

            @Override
            public String visit(Point point) {
                return "point";
            }

            @Override
            public String visit(Polygon polygon) {
                return "polygon";
            }

            @Override
            public String visit(Rectangle rectangle) {
                return "bbox";
            }
        });
    }

}
