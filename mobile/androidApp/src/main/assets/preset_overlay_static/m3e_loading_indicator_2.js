class MeasuredPolygon {
    #measurer;
    #cubics;
    features;
    constructor(measurer, features, cubics, outlineProgress) {
        if (outlineProgress.length !== cubics.length + 1) {
            throw new Error("Outline progress size is expected to be the cubics size + 1");
        }
        if (outlineProgress[0] !== 0) {
            throw new Error("First outline progress value is expected to be zero");
        }
        if (Math.abs(outlineProgress[outlineProgress.length - 1] - 1.0) > DistanceEpsilon) {
             throw new Error("Last outline progress value is expected to be one");
        }
        this.#measurer = measurer;
        this.features = features;
        if (DEBUG) {
            debugLog(LOG_TAG, `CTOR: cubics = ${cubics.join(", ")}\nCTOR: op = ${outlineProgress.join(", ")}`);
        }
        const measuredCubics = [];
        let startOutlineProgress = 0;
        for (let index = 0; index < cubics.length; index++) {
            if ((outlineProgress[index + 1] - outlineProgress[index]) > DistanceEpsilon) {
                measuredCubics.push(
                    new MeasuredCubic(
                        cubics[index],
                        startOutlineProgress,
                        outlineProgress[index + 1],
                        this.#measurer
                    )
                );
                startOutlineProgress = outlineProgress[index + 1];
            }
        }
        if (measuredCubics.length > 0) {
            measuredCubics[measuredCubics.length - 1].updateProgressRange(undefined, 1.0);
        }
        this.#cubics = measuredCubics;
    }
    cutAndShift(cuttingPoint) {
        if (cuttingPoint < 0 || cuttingPoint > 1) {
            throw new Error("Cutting point is expected to be between 0 and 1");
        }
        if (cuttingPoint < DistanceEpsilon) return this;
        const targetIndex = this.#cubics.findIndex(it =>
            cuttingPoint >= it.startOutlineProgress && cuttingPoint <= it.endOutlineProgress
        );
        if (targetIndex === -1) {
            if (Math.abs(cuttingPoint - 1.0) < DistanceEpsilon) {
                return this;
            }
            throw new Error(`Cutting point ${cuttingPoint} not found in any cubic range.`);
        }
        const target = this.#cubics[targetIndex];
        if (DEBUG) {
            this.#cubics.forEach((cubic, index) => debugLog(LOG_TAG, `cut&Shift | cubic #${index} : ${cubic} `));
            debugLog(LOG_TAG, `cut&Shift, cuttingPoint = ${cuttingPoint}, target = (${targetIndex}) ${target}`);
        }
        const [b1, b2] = target.cutAtProgress(cuttingPoint);
        if (DEBUG) debugLog(LOG_TAG, `Split | ${target} -> ${b1} & ${b2}`);
        const retCubics = [b2.cubic];
        for (let i = 1; i < this.#cubics.length; i++) {
            retCubics.push(this.#cubics[(i + targetIndex) % this.#cubics.length].cubic);
        }
        retCubics.push(b1.cubic);
        const retOutlineProgress = [0];
        for (let index = 1; index < retCubics.length; index++) {
            const cubicIndex = (targetIndex + index - 1) % this.#cubics.length;
            retOutlineProgress.push(
                positiveModulo(this.#cubics[cubicIndex].endOutlineProgress - cuttingPoint, 1.0)
            );
        }
        retOutlineProgress.push(1.0);
        const newFeatures = this.features.map(f =>
            new ProgressableFeature(
                positiveModulo(f.progress - cuttingPoint, 1.0),
                f.feature
            )
        );
        return new MeasuredPolygon(this.#measurer, newFeatures, retCubics, retOutlineProgress);
    }
    get size() { return this.#cubics.length; }
    get(index) { return this.#cubics[index]; }
    [Symbol.iterator]() { return this.#cubics[Symbol.iterator](); }
    static measurePolygon(measurer, polygon) {
        const cubics = [];
        const featureToCubic = [];
        for (const feature of polygon.features) {
            for (let cubicIndex = 0; cubicIndex < feature.cubics.length; cubicIndex++) {
                if (feature.isCorner && cubicIndex === Math.floor(feature.cubics.length / 2)) {
                    featureToCubic.push({ feature, index: cubics.length });
                }
                cubics.push(feature.cubics[cubicIndex]);
            }
        }
        const measures = [0];
        let totalMeasure = 0;
        for (const cubic of cubics) {
            const measure = measurer.measureCubic(cubic);
            if (measure < 0) {
                throw new Error("Measured cubic is expected to be greater or equal to zero");
            }
            totalMeasure += measure;
            measures.push(totalMeasure);
        }
        const outlineProgress = measures.map(m => totalMeasure === 0 ? 0 : m / totalMeasure);
        if(outlineProgress.length > 0) {
            outlineProgress[outlineProgress.length - 1] = 1.0; // Ensure it ends exactly at 1.0
        }
        if (DEBUG) debugLog(LOG_TAG, `Total size: ${totalMeasure}`);
        const features = featureToCubic.map(({ feature, index }) => {
            const progress = positiveModulo(
                (outlineProgress[index] + outlineProgress[index + 1]) / 2,
                1.0
            );
            return new ProgressableFeature(progress, feature);
        });
        return new MeasuredPolygon(measurer, features, cubics, outlineProgress);
    }
}
class Measurer {
    measureCubic(c) {
        throw new Error("Not implemented");
    }
    findCubicCutPoint(c, m) {
        throw new Error("Not implemented");
    }
}
class LengthMeasurer extends Measurer {
    #segments = 3;
    measureCubic(c) {
        return this.#closestProgressTo(c, Infinity).second;
    }
    findCubicCutPoint(c, m) {
        return this.#closestProgressTo(c, m).first;
    }
    #closestProgressTo(cubic, threshold) {
        let total = 0;
        let remainder = threshold;
        let prev = new Point(cubic.anchor0X, cubic.anchor0Y);
        for (let i = 1; i <= this.#segments; i++) {
            const progress = i / this.#segments;
            const point = cubic.pointOnCurve(progress);
            const segment = point.minus(prev).getDistance();
            if (segment >= remainder) {
                const p = progress - (1.0 - remainder / segment) / this.#segments;
                return { first: p, second: threshold };
            }
            remainder -= segment;
            total += segment;
            prev = point;
        }
        return { first: 1.0, second: total };
    }
}
const CornerRounding = { Unrounded: 0 };
class Feature {
    cubics;
    isCorner = false;
    constructor(cubics) {
        this.cubics = cubics;
    }
    transformed(f) {
        throw new Error("Not implemented");
    }
}
Feature.Corner = class Corner extends Feature {
    convex;
    isCorner = true;
    constructor(cubics, convex) {
        super(cubics);
        this.convex = convex;
    }
    transformed(f) {
        return new Feature.Corner(this.cubics.map(c => c.transformed(f)), this.convex);
    }
};
Feature.Edge = class Edge extends Feature {
    transformed(f) {
        return new Feature.Edge(this.cubics.map(c => c.transformed(f)));
    }
};
class RoundedPolygon {
    features;
    center;
    cubics;
    get centerX() { return this.center.x; }
    get centerY() { return this.center.y; }
    constructor(arg1, ...args) {
        let features, center;
        if (arg1 instanceof RoundedPolygon) {
            features = arg1.features;
            center = arg1.center;
        } else if (typeof arg1 === 'number') {
            const [
                radius = 1,
                centerX = 0,
                centerY = 0,
                rounding = 0,
                perVertexRounding = null
            ] = args;
            const vertices = verticesFromNumVerts(arg1, radius, centerX, centerY);
            ({ features, center } =
                computeFeaturesFromVertices(vertices, rounding, perVertexRounding, centerX, centerY));
        } else if (Array.isArray(arg1) && (arg1.length === 0 || arg1[0] instanceof Feature)) {
            const [centerX = NaN, centerY = NaN] = args;
            features = arg1;
            if (features.length < 2 && features.length > 0) throw new Error("Polygons must have at least 2 features");
            const vertices = [];
            for (const feature of features) {
                for (const cubic of feature.cubics) {
                    vertices.push(cubic.anchor0X, cubic.anchor0Y);
                }
            }
            const calculatedCenter = calculateCenter(vertices);
            const cX = !isNaN(centerX) ? centerX : calculatedCenter.x;
            const cY = !isNaN(centerY) ? centerY : calculatedCenter.y;
            center = new Point(cX, cY);
        } else if (arg1 instanceof Float32Array || Array.isArray(arg1)) {
            const [
                rounding = 0,
                perVertexRounding = null,
                centerX = NaN,
                centerY = NaN
            ] = args;
            ({ features, center } =
                computeFeaturesFromVertices(arg1, rounding, perVertexRounding, centerX, centerY));
        } else {
            throw new Error("Invalid arguments for RoundedPolygon constructor");
        }
        this.features = features;
        this.center = center;
        this.cubics = this.#flattenCubics(features, center);
        this.#validateContinuity();
    }
    #flattenCubics(features, center) {
        const cubics = [];
        if (features.length === 0) {
            cubics.push(new Cubic(new Float32Array([
                center.x, center.y, center.x, center.y,
                center.x, center.y, center.x, center.y
            ])));
            return cubics;
        }
        let firstCubic = null;
        let lastCubic = null;
        for (const feature of features) {
            for (const cubic of feature.cubics) {
                if (!cubic.zeroLength()) {
                    if (lastCubic) cubics.push(lastCubic);
                    lastCubic = cubic;
                    if (!firstCubic) firstCubic = cubic;
                } else if (lastCubic) {
                    const newPoints = lastCubic.points.slice();
                    newPoints[6] = cubic.anchor1X;
                    newPoints[7] = cubic.anchor1Y;
                    lastCubic = new Cubic(newPoints);
                }
            }
        }
        if (lastCubic && firstCubic) {
            cubics.push(new Cubic(new Float32Array([
                lastCubic.anchor0X, lastCubic.anchor0Y,
                lastCubic.control0X, lastCubic.control0Y,
                lastCubic.control1X, lastCubic.control1Y,
                firstCubic.anchor0X, firstCubic.anchor0Y,
            ])));
        }
        return cubics;
    }
    #validateContinuity() {
        if (this.cubics.length <= 1) return;
        let prevCubic = this.cubics[this.cubics.length - 1];
        for (let index = 0; index < this.cubics.length; index++) {
            const cubic = this.cubics[index];
            if (Math.abs(cubic.anchor0X - prevCubic.anchor1X) > DistanceEpsilon ||
                Math.abs(cubic.anchor0Y - prevCubic.anchor1Y) > DistanceEpsilon) {
                throw new Error(
                    "RoundedPolygon must be contiguous, with the anchor points of all curves " +
                    "matching the anchor points of the preceding and succeeding cubics"
                );
            }
            prevCubic = cubic;
        }
    }
    transformed(f) {
        const newCenter = f.transform(this.center.x, this.center.y);
        const newFeatures = this.features.map(feat => feat.transformed(f));
        return new RoundedPolygon(newFeatures, newCenter.first, newCenter.second);
    }
    normalized() {
        const bounds = this.calculateBounds();
        const width = bounds[2] - bounds[0];
        const height = bounds[3] - bounds[1];
        const side = Math.max(width, height);
        if (side === 0) return this;
        const offsetX = (side - width) / 2 - bounds[0];
        const offsetY = (side - height) / 2 - bounds[1];
        return this.transformed((x, y) => ({
            first: (x + offsetX) / side,
            second: (y + offsetY) / side
        }));
    }
    calculateMaxBounds(bounds = new Float32Array(4)) {
        if (bounds.length < 4) throw new Error("Required bounds size of 4");
        let maxDistSquared = 0;
        for (const cubic of this.cubics) {
            const anchorDistance = distanceSquared(cubic.anchor0X - this.centerX, cubic.anchor0Y - this.centerY);
            const middlePoint = cubic.pointOnCurve(0.5);
            const middleDistance = distanceSquared(middlePoint.x - this.centerX, middlePoint.y - this.centerY);
            maxDistSquared = Math.max(maxDistSquared, anchorDistance, middleDistance);
        }
        const dist = Math.sqrt(maxDistSquared);
        bounds[0] = this.centerX - dist;
        bounds[1] = this.centerY - dist;
        bounds[2] = this.centerX + dist;
        bounds[3] = this.centerY + dist;
        return bounds;
    }
    calculateBounds(bounds = new Float32Array(4), approximate = true) {
        if (bounds.length < 4) throw new Error("Required bounds size of 4");
        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
        const tempBounds = new Float32Array(4);
        for (const cubic of this.cubics) {
            cubic.calculateBounds(tempBounds, approximate);
            minX = Math.min(minX, tempBounds[0]);
            minY = Math.min(minY, tempBounds[1]);
            maxX = Math.max(maxX, tempBounds[2]);
            maxY = Math.max(maxY, tempBounds[3]);
        }
        bounds[0] = minX;
        bounds[1] = minY;
        bounds[2] = maxX;
        bounds[3] = maxY;
        return bounds;
    }
    equals(other) {
        if (this === other) return true;
        if (!(other instanceof RoundedPolygon)) return false;
        if (this.features.length !== other.features.length) return false;
        return JSON.stringify(this.features) === JSON.stringify(other.features);
    }
}
function calculateCenter(vertices) {
    let cumulativeX = 0, cumulativeY = 0;
    for (let i = 0; i < vertices.length; i += 2) {
        cumulativeX += vertices[i];
        cumulativeY += vertices[i + 1];
    }
    const numPoints = vertices.length / 2;
    return new Point(
        numPoints > 0 ? cumulativeX / numPoints : 0,
        numPoints > 0 ? cumulativeY / numPoints : 0
    );
}
function verticesFromNumVerts(numVertices, radius, centerX, centerY) {
    const result = new Float32Array(numVertices * 2);
    const centerPoint = new Point(centerX, centerY);
    for (let i = 0; i < numVertices; i++) {
        const angle = (FloatPi / numVertices * 2 * i);
        const vertex = radialToCartesian(radius, angle).plus(centerPoint);
        result[i * 2] = vertex.x;
        result[i * 2 + 1] = vertex.y;
    }
    return result;
}
function computeFeaturesFromVertices(vertices, rounding, perVertexRounding, centerX, centerY) {
    if (vertices.length < 6) throw new Error("Polygons must have at least 3 vertices");
    if (vertices.length % 2 !== 0) throw new Error("The vertices array should have even size");
    const numVerts = vertices.length / 2;
    if (perVertexRounding && perVertexRounding.length !== numVerts) {
        throw new Error("perVertexRounding list size must match the number of vertices");
    }
    const roundedCorners = [];
    for (let i = 0; i < numVerts; i++) {
        const vtxRounding = perVertexRounding ? perVertexRounding[i] : rounding;
        const prevI = (i + numVerts - 1) % numVerts;
        const nextI = (i + 1) % numVerts;
        roundedCorners.push(
            new RoundedCorner(
                new Point(vertices[prevI * 2], vertices[prevI * 2 + 1]),
                new Point(vertices[i * 2], vertices[i * 2 + 1]),
                new Point(vertices[nextI * 2], vertices[nextI * 2 + 1]),
                vtxRounding,
            )
        );
    }
    const cutAdjusts = roundedCorners.map((rc, i) => {
        const nextRc = roundedCorners[(i + 1) % numVerts];
        const expectedRoundCut = rc.expectedRoundCut + nextRc.expectedRoundCut;
        const expectedCut = rc.expectedCut + nextRc.expectedCut;
        const sideSize = distance(
            vertices[i * 2] - vertices[((i + 1) % numVerts) * 2],
            vertices[i * 2 + 1] - vertices[((i + 1) % numVerts) * 2 + 1]
        );
        if (expectedRoundCut > sideSize) {
            return { roundRatio: sideSize / expectedRoundCut, smoothRatio: 0 };
        } else if (expectedCut > sideSize) {
            return { roundRatio: 1, smoothRatio: (sideSize - expectedRoundCut) / (expectedCut - expectedRoundCut) };
        } else {
            return { roundRatio: 1, smoothRatio: 1 };
        }
    });
    const corners = [];
    for (let i = 0; i < numVerts; i++) {
        const allowedCuts = [];
        for (const delta of [0, 1]) {
            const adjust = cutAdjusts[(i + numVerts - 1 + delta) % numVerts];
            allowedCuts.push(
                roundedCorners[i].expectedRoundCut * adjust.roundRatio +
                (roundedCorners[i].expectedCut - roundedCorners[i].expectedRoundCut) * adjust.smoothRatio
            );
        }
        corners.push(roundedCorners[i].getCubics(allowedCuts[0], allowedCuts[1]));
    }
    const tempFeatures = [];
    for (let i = 0; i < numVerts; i++) {
        const prevI = (i + numVerts - 1) % numVerts;
        const nextI = (i + 1) % numVerts;
        const currVertex = new Point(vertices[i * 2], vertices[i * 2 + 1]);
        const prevVertex = new Point(vertices[prevI * 2], vertices[prevI * 2 + 1]);
        const nextVertex = new Point(vertices[nextI * 2], vertices[nextI * 2 + 1]);
        const isConvex = convex(prevVertex, currVertex, nextVertex);
        tempFeatures.push(new Feature.Corner(corners[i], isConvex));
        const lastOfCorner = corners[i][corners[i].length - 1];
        const firstOfNextCorner = corners[(i + 1) % numVerts][0];
        tempFeatures.push(new Feature.Edge([
            Cubic.straightLine(
                lastOfCorner.anchor1X, lastOfCorner.anchor1Y,
                firstOfNextCorner.anchor0X, firstOfNextCorner.anchor0Y
            )
        ]));
    }
    const center = (isNaN(centerX) || isNaN(centerY)) ?
        calculateCenter(vertices) :
        new Point(centerX, centerY);
    return { features: tempFeatures, center };
}
class RoundedCorner {
    constructor(p0, p1, p2, rounding) {
        this.p0 = p0;
        this.p1 = p1;
        this.p2 = p2;
        this.rounding = rounding || 0;
        const v01 = p0.minus(p1);
        const v21 = p2.minus(p1);
        const d01 = v01.getDistance();
        const d21 = v21.getDistance();
        if (d01 > 0 && d21 > 0) {
            this.d1 = v01.times(1 / d01);
            this.d2 = v21.times(1 / d21);
            this.cornerRadius = (typeof this.rounding === 'number') ? this.rounding : (this.rounding.radius || 0);
            this.smoothing = (typeof this.rounding === 'number') ? 0 : (this.rounding.smoothing || 0);
            this.cosAngle = this.d1.dotProduct(this.d2);
            this.sinAngle = Math.sqrt(1 - square(this.cosAngle));
            this.expectedRoundCut = (this.sinAngle > 1e-3) ?
                this.cornerRadius * (this.cosAngle + 1) / this.sinAngle : 0;
        } else {
            this.d1 = Zero; this.d2 = Zero; this.cornerRadius = 0;
            this.smoothing = 0; this.cosAngle = 0; this.sinAngle = 0;
            this.expectedRoundCut = 0;
        }
    }
    get expectedCut() {
        return (1 + this.smoothing) * this.expectedRoundCut;
    }
    getCubics(allowedCut0, allowedCut1) {
        const allowedCut = Math.min(allowedCut0, allowedCut1);
        if (this.expectedRoundCut < DistanceEpsilon || allowedCut < DistanceEpsilon || this.cornerRadius < DistanceEpsilon) {
            return [Cubic.empty(this.p1.x, this.p1.y)];
        }
        const actualRoundCut = Math.min(allowedCut, this.expectedRoundCut);
        const actualSmoothing0 = this.#calculateActualSmoothingValue(allowedCut0);
        const actualSmoothing1 = this.#calculateActualSmoothingValue(allowedCut1);
        const actualR = this.cornerRadius * actualRoundCut / this.expectedRoundCut;
        const centerDistance = Math.sqrt(square(actualR) + square(actualRoundCut));
        const center = this.p1.plus(this.d1.plus(this.d2).times(0.5).getDirection().times(centerDistance));
        const circleIntersection0 = this.p1.plus(this.d1.times(actualRoundCut));
        const circleIntersection2 = this.p1.plus(this.d2.times(actualRoundCut));
        const flanking0 = this.#computeFlankingCurve(
            actualRoundCut, actualSmoothing0, this.p1, this.p0,
            circleIntersection0, circleIntersection2, center, actualR
        );
        const flanking2 = this.#computeFlankingCurve(
            actualRoundCut, actualSmoothing1, this.p1, this.p2,
            circleIntersection2, circleIntersection0, center, actualR
        ).reverse();
        return [
            flanking0,
            Cubic.circularArc(
                center.x, center.y,
                flanking0.anchor1X, flanking0.anchor1Y,
                flanking2.anchor0X, flanking2.anchor0Y
            ),
            flanking2,
        ];
    }
    #calculateActualSmoothingValue(allowedCut) {
        if (allowedCut > this.expectedCut) {
            return this.smoothing;
        } else if (allowedCut > this.expectedRoundCut) {
            const denom = this.expectedCut - this.expectedRoundCut;
            return this.smoothing * (denom > 0 ? (allowedCut - this.expectedRoundCut) / denom : 0);
        } else {
            return 0;
        }
    }
    #computeFlankingCurve(
        actualRoundCut, actualSmoothingValue, corner, sideStart,
        circleSegmentIntersection, otherCircleSegmentIntersection,
        circleCenter, actualR
    ) {
        const sideDirection = sideStart.minus(corner).getDirection();
        const curveStart = corner.plus(sideDirection.times(actualRoundCut * (1 + actualSmoothingValue)));
        const p = circleSegmentIntersection.times(1 - actualSmoothingValue).plus(
            circleSegmentIntersection.plus(otherCircleSegmentIntersection).times(0.5 * actualSmoothingValue)
        );
        const curveEnd = circleCenter.plus(
            directionVector(p.x - circleCenter.x, p.y - circleCenter.y).times(actualR)
        );
        const circleTangent = curveEnd.minus(circleCenter).rotate90();
        const anchorEnd = lineIntersection(sideStart, sideDirection, curveEnd, circleTangent) ||
                          circleSegmentIntersection;
        const anchorStart = curveStart.plus(anchorEnd.times(2)).times(1 / 3);
        return new Cubic(new Float32Array([
            curveStart.x, curveStart.y,
            anchorStart.x, anchorStart.y,
            anchorEnd.x, anchorEnd.y,
            curveEnd.x, curveEnd.y
        ]));
    }
}
function lineIntersection(p0, d0, p1, d1) {
    const rotatedD1 = d1.rotate90();
    const den = d0.dotProduct(rotatedD1);
    if (Math.abs(den) < DistanceEpsilon) return null;
    const num = p1.minus(p0).dotProduct(rotatedD1);
    if (Math.abs(den) < DistanceEpsilon * Math.abs(num)) return null;
    const k = num / den;
    return p0.plus(d0.times(k));
}
class Morph {
    #start;
    #end;
    #morphMatch;
    constructor(start, end) {
        this.#start = start;
        this.#end = end;
        this.#morphMatch = Morph.match(start, end);
    }
    get morphMatch() {
        return this.#morphMatch;
    }
    bounds(progress) {
        if (this.#morphMatch.length === 0) {
            return [0, 0, 0, 0];
        }
        let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
        for (const pair of this.#morphMatch) {
            const points = new Float32Array(8);
            for (let j = 0; j < 8; j++) {
                points[j] = interpolate(pair.first.points[j], pair.second.points[j], progress);
            }
            for (let i = 0; i < 8; i += 2) {
                const x = points[i];
                const y = points[i + 1];
                minX = Math.min(minX, x);
                maxX = Math.max(maxX, x);
                minY = Math.min(minY, y);
                maxY = Math.max(maxY, y);
            }
        }
        const bounds = [minX, minY, maxX, maxY];
        bounds[3] = Math.max(maxY, bounds[3]);
        return bounds;
    }
    asCubics(progress) {
        const result = [];
        if (this.#morphMatch.length === 0) {
            return result;
        }
        let firstCubic = null;
        let lastCubic = null;
        for (let i = 0; i < this.#morphMatch.length; i++) {
            const pair = this.#morphMatch[i];
            const points = new Float32Array(8);
            for (let j = 0; j < 8; j++) {
                points[j] = interpolate(pair.first.points[j], pair.second.points[j], progress);
            }
            const cubic = new Cubic(points);
            if (firstCubic === null) {
                firstCubic = cubic;
            }
            if (lastCubic !== null) {
                result.push(lastCubic);
            }
            lastCubic = cubic;
        }
        if (lastCubic !== null && firstCubic !== null) {
            result.push(
                createCubic(
                    lastCubic.anchor0X,
                    lastCubic.anchor0Y,
                    lastCubic.control0X,
                    lastCubic.control0Y,
                    lastCubic.control1X,
                    lastCubic.control1Y,
                    firstCubic.anchor0X,
                    firstCubic.anchor0Y
                )
            );
        }
        return result;
    }
    forEachCubic(progress, callback, mutableCubic = new MutableCubic()) {
        for (let i = 0; i < this.#morphMatch.length; i++) {
            const pair = this.#morphMatch[i];
            mutableCubic.interpolate(pair.first, pair.second, progress);
            callback(mutableCubic);
        }
    }
    static match(p1, p2) {
        const measuredPolygon1 = MeasuredPolygon.measurePolygon(new LengthMeasurer(), p1);
        const measuredPolygon2 = MeasuredPolygon.measurePolygon(new LengthMeasurer(), p2);
        const features1 = measuredPolygon1.features;
        const features2 = measuredPolygon2.features;
        const doubleMapper = featureMapper(features1, features2);
        const polygon2CutPoint = doubleMapper.map(0);
        if (DEBUG) debugLog(LOG_TAG, `polygon2CutPoint = ${polygon2CutPoint}`);
        const bs1 = measuredPolygon1;
        const bs2 = measuredPolygon2.cutAndShift(polygon2CutPoint);
        if (DEBUG) {
            for (let index = 0; index < bs1.size; index++) {
                const b1 = bs1.get(index);
                debugLog(LOG_TAG, `bs1[${index}] = ${b1.startOutlineProgress} .. ${b1.endOutlineProgress}`);
            }
            for (let index = 0; index < bs2.size; index++) {
                const b2 = bs2.get(index);
                debugLog(LOG_TAG, `bs2[${index}] = ${b2.startOutlineProgress} .. ${b2.endOutlineProgress}`);
            }
        }
        const ret = [];
        let i1 = 0;
        let i2 = 0;
        let b1 = bs1.get(i1++);
        let b2 = bs2.get(i2++);
        while (b1 && b2) {
            const b1a = (i1 === bs1.size) ? 1.0 : b1.endOutlineProgress;
            const b2a = (i2 === bs2.size) ? 1.0 :
                doubleMapper.mapBack(
                    positiveModulo(b2.endOutlineProgress + polygon2CutPoint, 1.0)
                );
            const minb = Math.min(b1a, b2a);
            if (DEBUG) debugLog(LOG_TAG, `${b1a} ${b2a} | ${minb}`);
            let seg1, newb1;
            if (b1a > minb + AngleEpsilon) {
                if (DEBUG) debugLog(LOG_TAG, "Cut 1");
                [seg1, newb1] = b1.cutAtProgress(minb);
            } else {
                seg1 = b1;
                newb1 = bs1.get(i1++);
            }
            let seg2, newb2;
            if (b2a > minb + AngleEpsilon) {
                if (DEBUG) debugLog(LOG_TAG, "Cut 2");
                [seg2, newb2] = b2.cutAtProgress(
                    positiveModulo(doubleMapper.map(minb) - polygon2CutPoint, 1.0)
                );
            } else {
                seg2 = b2;
                newb2 = bs2.get(i2++);
            }
            ret.push({ first: seg1.cubic, second: seg2.cubic });
            b1 = newb1;
            b2 = newb2;
        }
        return ret;
    }
}

window.RoundedPolygon=RoundedPolygon;window.Morph=Morph;
