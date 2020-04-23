/*
 * Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
 * or more contributor license agreements. Licensed under the Elastic License;
 * you may not use this file except in compliance with the Elastic License.
 */

package org.elasticsearch.xpack.spatial.index.fielddata;

import org.elasticsearch.common.geo.GeoUtils;
import org.elasticsearch.geometry.Circle;
import org.elasticsearch.geometry.Geometry;
import org.elasticsearch.geometry.GeometryCollection;
import org.elasticsearch.geometry.GeometryVisitor;
import org.elasticsearch.geometry.Line;
import org.elasticsearch.geometry.LinearRing;
import org.elasticsearch.geometry.MultiLine;
import org.elasticsearch.geometry.MultiPoint;
import org.elasticsearch.geometry.MultiPolygon;
import org.elasticsearch.geometry.Point;
import org.elasticsearch.geometry.Polygon;
import org.elasticsearch.geometry.Rectangle;
import org.elasticsearch.search.aggregations.metrics.CompensatedSum;

/**
 * This class keeps a running Kahan-sum of coordinates
 * that are to be averaged in {@code TriangleTreeWriter} for use
 * as the centroid of a shape.
 */
public class CentroidCalculator {
    CompensatedSum compSumX;
    CompensatedSum compSumY;
    CompensatedSum compSumWeight;
    private CentroidCalculatorVisitor visitor;
    private DimensionalShapeType dimensionalShapeType;

    public CentroidCalculator(Geometry geometry) {
        this.compSumX = new CompensatedSum(0, 0);
        this.compSumY = new CompensatedSum(0, 0);
        this.compSumWeight = new CompensatedSum(0, 0);
        this.dimensionalShapeType = null;
        this.visitor = new CentroidCalculatorVisitor(this);
        geometry.visit(visitor);
        this.dimensionalShapeType = visitor.calculator.dimensionalShapeType;
    }

    /**
     * adds a single coordinate to the running sum and count of coordinates
     * for centroid calculation
     *  @param x the x-coordinate of the point
     * @param y the y-coordinate of the point
     * @param weight the associated weight of the coordinate
     */
    private void addCoordinate(double x, double y, double weight, DimensionalShapeType dimensionalShapeType) {
        // x and y can be infinite due to really small areas and rounding problems
        if (Double.isFinite(x) && Double.isFinite(y)) {
            if (this.dimensionalShapeType == null || this.dimensionalShapeType == dimensionalShapeType) {
                compSumX.add(x * weight);
                compSumY.add(y * weight);
                compSumWeight.add(weight);
                this.dimensionalShapeType = dimensionalShapeType;
            } else if (dimensionalShapeType.compareTo(this.dimensionalShapeType) > 0) {
                // reset counters
                compSumX.reset(x * weight, 0);
                compSumY.reset(y * weight, 0);
                compSumWeight.reset(weight, 0);
                this.dimensionalShapeType = dimensionalShapeType;
            }
        }
    }

    /**
     * Adjusts the existing calculator to add the running sum and count
     * from another {@link CentroidCalculator}. This is used to keep
     * a running count of points from different sub-shapes of a single
     * geo-shape field
     *
     * @param otherCalculator the other centroid calculator to add from
     */
    public void addFrom(CentroidCalculator otherCalculator) {
        int compared = dimensionalShapeType.compareTo(otherCalculator.dimensionalShapeType);
        if (compared < 0) {
            dimensionalShapeType = otherCalculator.dimensionalShapeType;
            this.compSumX = otherCalculator.compSumX;
            this.compSumY = otherCalculator.compSumY;
            this.compSumWeight = otherCalculator.compSumWeight;

        } else if (compared == 0) {
            this.compSumX.add(otherCalculator.compSumX.value());
            this.compSumY.add(otherCalculator.compSumY.value());
            this.compSumWeight.add(otherCalculator.compSumWeight.value());
        } // else (compared > 0) do not modify centroid calculation since otherCalculator is of lower dimension than this calculator
    }

    /**
     * @return the x-coordinate centroid
     */
    public double getX() {
        // normalization required due to floating point precision errors
        return GeoUtils.normalizeLon(compSumX.value() / compSumWeight.value());
    }

    /**
     * @return the y-coordinate centroid
     */
    public double getY() {
        // normalization required due to floating point precision errors
        return GeoUtils.normalizeLat(compSumY.value() / compSumWeight.value());
    }

    /**
     * @return the sum of all the weighted coordinates summed in the calculator
     */
    public double sumWeight() {
        return compSumWeight.value();
    }

    /**
     * @return the highest dimensional shape type summed in the calculator
     */
    public DimensionalShapeType getDimensionalShapeType() {
        return dimensionalShapeType;
    }

    private static class CentroidCalculatorVisitor implements GeometryVisitor<Void, IllegalArgumentException> {

        private final CentroidCalculator calculator;

        private CentroidCalculatorVisitor(CentroidCalculator calculator) {
            this.calculator = calculator;
        }

        @Override
        public Void visit(Circle circle) {
            throw new IllegalArgumentException("invalid shape type found [Circle] while calculating centroid");
        }

        @Override
        public Void visit(GeometryCollection<?> collection) {
            for (Geometry shape : collection) {
                shape.visit(this);
            }
            return null;
        }

        @Override
        public Void visit(Line line) {
            if (calculator.dimensionalShapeType != DimensionalShapeType.POLYGON) {
                visitLine(line.length(), line::getX, line::getY);
            }
            return null;
        }

        @Override
        public Void visit(LinearRing ring) {
            throw new IllegalArgumentException("invalid shape type found [LinearRing] while calculating centroid");
        }


        @Override
        public Void visit(MultiLine multiLine) {
            if (calculator.getDimensionalShapeType() != DimensionalShapeType.POLYGON) {
                for (Line line : multiLine) {
                    visit(line);
                }
            }
            return null;
        }

        @Override
        public Void visit(MultiPoint multiPoint) {
            if (calculator.getDimensionalShapeType() == null || calculator.getDimensionalShapeType() == DimensionalShapeType.POINT) {
                for (Point point : multiPoint) {
                    visit(point);
                }
            }
            return null;
        }

        @Override
        public Void visit(MultiPolygon multiPolygon) {
            for (Polygon polygon : multiPolygon) {
                visit(polygon);
            }
            return null;
        }

        @Override
        public Void visit(Point point) {
            if (calculator.getDimensionalShapeType() == null || calculator.getDimensionalShapeType() == DimensionalShapeType.POINT) {
                visitPoint(point.getX(), point.getY());
            }
            return null;
        }

        @Override
        public Void visit(Polygon polygon) {
            // check area of polygon

            double[] centroidX = new double[1 + polygon.getNumberOfHoles()];
            double[] centroidY = new double[1 + polygon.getNumberOfHoles()];
            double[] weight = new double[1 + polygon.getNumberOfHoles()];
            visitLinearRing(polygon.getPolygon().length(), polygon.getPolygon()::getX, polygon.getPolygon()::getY, false,
                centroidX, centroidY, weight, 0);
            for (int i = 0; i < polygon.getNumberOfHoles(); i++) {
                visitLinearRing(polygon.getHole(i).length(), polygon.getHole(i)::getX, polygon.getHole(i)::getY, true,
                    centroidX, centroidY, weight, i + 1);
            }

            double sumWeight = 0;
            for (double w : weight) {
                sumWeight += w;
            }

            if (sumWeight == 0 && calculator.dimensionalShapeType != DimensionalShapeType.POLYGON) {
                visitLine(polygon.getPolygon().length(), polygon.getPolygon()::getX, polygon.getPolygon()::getY);
            } else {
                for (int i = 0; i < 1 + polygon.getNumberOfHoles(); i++) {
                    calculator.addCoordinate(centroidX[i], centroidY[i], weight[i], DimensionalShapeType.POLYGON);
                }
            }

            return null;
        }

        @Override
        public Void visit(Rectangle rectangle) {
            double sumX = rectangle.getMaxX() + rectangle.getMinX();
            double sumY = rectangle.getMaxY() + rectangle.getMinY();
            double diffX = rectangle.getMaxX() - rectangle.getMinX();
            double diffY = rectangle.getMaxY() - rectangle.getMinY();
            if (diffX != 0 && diffY != 0) {
                calculator.addCoordinate(sumX / 2, sumY / 2, Math.abs(diffX * diffY), DimensionalShapeType.POLYGON);
            } else if (diffX != 0) {
                calculator.addCoordinate(sumX / 2, rectangle.getMinY(), diffX, DimensionalShapeType.LINE);
            } else if (diffY != 0) {
                calculator.addCoordinate(rectangle.getMinX(), sumY / 2, diffY, DimensionalShapeType.LINE);
            } else {
                visitPoint(rectangle.getMinX(), rectangle.getMinY());
            }
            return null;
        }


        private void visitPoint(double x, double y) {
            calculator.addCoordinate(x, y, 1.0, DimensionalShapeType.POINT);
        }

        private void visitLine(int length, CoordinateSupplier x, CoordinateSupplier y) {
            // check line has length
            double originDiffX = x.get(0) - x.get(1);
            double originDiffY = y.get(0) - y.get(1);
            if (originDiffX != 0 || originDiffY != 0) {
                // a line's centroid is calculated by summing the center of each
                // line segment weighted by the line segment's length in degrees
                for (int i = 0; i < length - 1; i++) {
                    double diffX = x.get(i) - x.get(i + 1);
                    double diffY = y.get(i) - y.get(i + 1);
                    double xAvg = (x.get(i) + x.get(i + 1)) / 2;
                    double yAvg = (y.get(i) + y.get(i + 1)) / 2;
                    double weight = Math.sqrt(diffX * diffX + diffY * diffY);
                    calculator.addCoordinate(xAvg, yAvg, weight, DimensionalShapeType.LINE);
                }
            } else {
                visitPoint(x.get(0), y.get(0));
            }
        }

        private void visitLinearRing(int length, CoordinateSupplier x, CoordinateSupplier y, boolean isHole,
                                       double[] centroidX, double[] centroidY, double[] weight, int idx) {
            // implementation of calculation defined in
            // https://www.seas.upenn.edu/~sys502/extra_materials/Polygon%20Area%20and%20Centroid.pdf
            //
            // centroid of a ring is a weighted coordinate based on the ring's area.
            // the sign of the area is positive for the outer-shell of a polygon and negative for the holes

            int sign = isHole ? -1 : 1;
            double totalRingArea = 0.0;
            for (int i = 0; i < length - 1; i++) {
                totalRingArea += (x.get(i) * y.get(i + 1)) - (x.get(i + 1) * y.get(i));
            }
            totalRingArea = totalRingArea / 2;

            double sumX = 0.0;
            double sumY = 0.0;
            for (int i = 0; i < length - 1; i++) {
                double twiceArea = (x.get(i) * y.get(i + 1)) - (x.get(i + 1) * y.get(i));
                sumX += twiceArea * (x.get(i) + x.get(i + 1));
                sumY += twiceArea * (y.get(i) + y.get(i + 1));
            }
            centroidX[idx] = sumX / (6 * totalRingArea);
            centroidY[idx] = sumY / (6 * totalRingArea);
            weight[idx] = sign * Math.abs(totalRingArea);
        }
    }

    @FunctionalInterface
    private interface CoordinateSupplier {
        double get(int idx);
    }
}
