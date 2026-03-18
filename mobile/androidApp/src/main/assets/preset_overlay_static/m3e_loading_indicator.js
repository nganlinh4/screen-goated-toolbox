
class Point {
    constructor(x, y) {
        this.x = x;
        this.y = y;
    }
    rotate90() {
        return new Point(-this.y, this.x);
    }
    dotProduct(otherX, otherY) {
        if (otherX instanceof Point) {
            return this.x * otherX.x + this.y * otherX.y;
        }
        return this.x * otherX + this.y * otherY;
    }
    getDistance() {
        return Math.sqrt(this.x * this.x + this.y * this.y);
    }
    plus(other) {
        return new Point(this.x + other.x, this.y + other.y);
    }
    minus(other) {
        return new Point(this.x - other.x, this.y - other.y);
    }
    times(scalar) {
        return new Point(this.x * scalar, this.y * scalar);
    }
    clockwise(other) {
        return this.x * other.y - this.y * other.x >= 0;
    }
    add(other) { return this.plus(other); }
    subtract(other) { return this.minus(other); }
    scale(factor) { return this.times(factor); }
    getDirection() {
        const d = this.getDistance();
        return d > DistanceEpsilon ? this.scale(1 / d) : new Point(0, 0);
    }
    transformed(f) {
        const result = f(this.x, this.y);
        return new Point(result.x, result.y);
    }
    equals(other) {
        if (!other) return false;
        return Math.abs(this.x - other.x) < DistanceEpsilon && Math.abs(this.y - other.y) < DistanceEpsilon;
    }
}
function distance(x, y) {
    return Math.sqrt(x * x + y * y);
}
function distanceSquared(x, y) {
    return x * x + y * y;
}
function directionVector(x, y) {
    if (arguments.length === 2) {
        const d = distance(x, y);
        if (d <= 0) {
            throw new Error("Required distance greater than zero");
        }
        return new Point(x / d, y / d);
    }
    const angleRadians = x;
    return new Point(Math.cos(angleRadians), Math.sin(angleRadians));
}
const Zero = new Point(0, 0);
function radialToCartesian(radius, angleRadians, center = Zero) {
    return directionVector(angleRadians).times(radius).plus(center);
}
const DistanceEpsilon = 1e-4;
const AngleEpsilon = 1e-6;
const RelaxedDistanceEpsilon = 5e-3;
const FloatPi = Math.PI;
const TwoPi = 2 * Math.PI;
function square(x) {
    return x * x;
}
function interpolate(start, stop, fraction) {
    return (1 - fraction) * start + fraction * stop;
}
function positiveModulo(num, mod) {
    return ((num % mod) + mod) % mod;
}
function collinearIsh(aX, aY, bX, bY, cX, cY, tolerance = DistanceEpsilon) {
    const ab = new Point(bX - aX, bY - aY).rotate90();
    const ac = new Point(cX - aX, cY - aY);
    const dotProduct = Math.abs(ab.dotProduct(ac));
    const relativeTolerance = tolerance * ab.getDistance() * ac.getDistance();
    return dotProduct < tolerance || dotProduct < relativeTolerance;
}
function convex(previous, current, next) {
    return current.minus(previous).clockwise(next.minus(current));
}
function findMinimum(v0, v1, tolerance = 1e-3, f) {
    let a = v0;
    let b = v1;
    while (b - a > tolerance) {
        const c1 = (2 * a + b) / 3;
        const c2 = (2 * b + a) / 3;
        if (f(c1) < f(c2)) {
            b = c2;
        } else {
            a = c1;
        }
    }
    return (a + b) / 2;
}
const DEBUG = false;
function debugLog(tag, messageFactory) {
    if (DEBUG) {
        console.log(`${tag}: ${messageFactory()}`);
    }
}
class Cubic {
    points;
    constructor(points = new Float32Array(8), anchor0Y, control0X, control0Y, control1X, control1Y, anchor1X, anchor1Y) {
        if (typeof points === 'number' && arguments.length === 8) {
            this.points = new Float32Array([
                points, anchor0Y, control0X, control0Y,
                control1X, control1Y, anchor1X, anchor1Y
            ]);
        } else {
            if (points.length !== 8) {
                throw new Error("Points array size should be 8");
            }
            this.points = points instanceof Float32Array ? points : new Float32Array(points);
        }
    }
    get anchor0X() { return this.points[0]; }
    get anchor0Y() { return this.points[1]; }
    get control0X() { return this.points[2]; }
    get control0Y() { return this.points[3]; }
    get control1X() { return this.points[4]; }
    get control1Y() { return this.points[5]; }
    get anchor1X() { return this.points[6]; }
    get anchor1Y() { return this.points[7]; }
    pointOnCurve(t) {
        const u = 1 - t;
        const u2 = u * u;
        const u3 = u2 * u;
        const t2 = t * t;
        const t3 = t2 * t;
        return new Point(
            this.anchor0X * u3 +
            this.control0X * (3 * t * u2) +
            this.control1X * (3 * t2 * u) +
            this.anchor1X * t3,
            this.anchor0Y * u3 +
            this.control0Y * (3 * t * u2) +
            this.control1Y * (3 * t2 * u) +
            this.anchor1Y * t3,
        );
    }
    zeroLength() {
        return Math.abs(this.anchor0X - this.anchor1X) < DistanceEpsilon &&
               Math.abs(this.anchor0Y - this.anchor1Y) < DistanceEpsilon;
    }
    convexTo(next) {
        const prevVertex = new Point(this.anchor0X, this.anchor0Y);
        const currVertex = new Point(this.anchor1X, this.anchor1Y);
        const nextVertex = new Point(next.anchor1X, next.anchor1Y);
        return convex(prevVertex, currVertex, nextVertex);
    }
    zeroIsh(value) {
        return Math.abs(value) < DistanceEpsilon;
    }
    calculateBounds(bounds = new Float32Array(4), approximate = false) {
        if (this.zeroLength()) {
            bounds[0] = this.anchor0X;
            bounds[1] = this.anchor0Y;
            bounds[2] = this.anchor0X;
            bounds[3] = this.anchor0Y;
            return;
        }
        let minX = Math.min(this.anchor0X, this.anchor1X);
        let minY = Math.min(this.anchor0Y, this.anchor1Y);
        let maxX = Math.max(this.anchor0X, this.anchor1X);
        let maxY = Math.max(this.anchor0Y, this.anchor1Y);
        if (approximate) {
            bounds[0] = Math.min(minX, this.control0X, this.control1X);
            bounds[1] = Math.min(minY, this.control0Y, this.control1Y);
            bounds[2] = Math.max(maxX, this.control0X, this.control1X);
            bounds[3] = Math.max(maxY, this.control0Y, this.control1Y);
            return;
        }
        const xa = -this.anchor0X + 3 * this.control0X - 3 * this.control1X + this.anchor1X;
        const xb = 2 * this.anchor0X - 4 * this.control0X + 2 * this.control1X;
        const xc = -this.anchor0X + this.control0X;
        if (this.zeroIsh(xa)) {
            if (xb !== 0) {
                const t = -xc / xb;
                if (t >= 0 && t <= 1) {
                    const x = this.pointOnCurve(t).x;
                    minX = Math.min(minX, x);
                    maxX = Math.max(maxX, x);
                }
            }
        } else {
            const xs = xb * xb - 4 * xa * xc;
            if (xs >= 0) {
                const sqrtXs = Math.sqrt(xs);
                const t1 = (-xb + sqrtXs) / (2 * xa);
                if (t1 >= 0 && t1 <= 1) {
                    const x = this.pointOnCurve(t1).x;
                    minX = Math.min(minX, x);
                    maxX = Math.max(maxX, x);
                }
                const t2 = (-xb - sqrtXs) / (2 * xa);
                if (t2 >= 0 && t2 <= 1) {
                    const x = this.pointOnCurve(t2).x;
                    minX = Math.min(minX, x);
                    maxX = Math.max(maxX, x);
                }
            }
        }
        const ya = -this.anchor0Y + 3 * this.control0Y - 3 * this.control1Y + this.anchor1Y;
        const yb = 2 * this.anchor0Y - 4 * this.control0Y + 2 * this.control1Y;
        const yc = -this.anchor0Y + this.control0Y;
        if (this.zeroIsh(ya)) {
            if (yb !== 0) {
                const t = -yc / yb;
                if (t >= 0 && t <= 1) {
                    const y = this.pointOnCurve(t).y;
                    minY = Math.min(minY, y);
                    maxY = Math.max(maxY, y);
                }
            }
        } else {
            const ys = yb * yb - 4 * ya * yc;
            if (ys >= 0) {
                const sqrtYs = Math.sqrt(ys);
                const t1 = (-yb + sqrtYs) / (2 * ya);
                if (t1 >= 0 && t1 <= 1) {
                    const y = this.pointOnCurve(t1).y;
                    minY = Math.min(minY, y);
                    maxY = Math.max(maxY, y);
                }
                const t2 = (-yb - sqrtYs) / (2 * ya);
                if (t2 >= 0 && t2 <= 1) {
                    const y = this.pointOnCurve(t2).y;
                    minY = Math.min(minY, y);
                    maxY = Math.max(maxY, y);
                }
            }
        }
        bounds[0] = minX;
        bounds[1] = minY;
        bounds[2] = maxX;
        bounds[3] = maxY;
    }
    split(t) {
        const u = 1 - t;
        const p = this.pointOnCurve(t);
        const c1 = createCubic(
            this.anchor0X, this.anchor0Y,
            this.anchor0X * u + this.control0X * t, this.anchor0Y * u + this.control0Y * t,
            this.anchor0X * u * u + this.control0X * 2 * u * t + this.control1X * t * t,
            this.anchor0Y * u * u + this.control0Y * 2 * u * t + this.control1Y * t * t,
            p.x, p.y
        );
        const c2 = createCubic(
            p.x, p.y,
            this.control0X * u * u + this.control1X * 2 * u * t + this.anchor1X * t * t,
            this.control0Y * u * u + this.control1Y * 2 * u * t + this.anchor1Y * t * t,
            this.control1X * u + this.anchor1X * t, this.control1Y * u + this.anchor1Y * t,
            this.anchor1X, this.anchor1Y
        );
        return [c1, c2];
    }
    reverse() {
        return createCubic(
            this.anchor1X, this.anchor1Y, this.control1X, this.control1Y,
            this.control0X, this.control0Y, this.anchor0X, this.anchor0Y
        );
    }
    plus(o) { return new Cubic(this.points.map((p, i) => p + o.points[i])); }
    times(x) { return new Cubic(this.points.map(p => p * x)); }
    div(x) { return this.times(1 / x); }
    toString() {
        return `anchor0: (${this.anchor0X}, ${this.anchor0Y}) control0: (${this.control0X}, ${this.control0Y}), ` +
               `control1: (${this.control1X}, ${this.control1Y}), anchor1: (${this.anchor1X}, ${this.anchor1Y})`;
    }
    equals(other) {
        if (this === other) return true;
        if (!(other instanceof Cubic)) return false;
        for (let i = 0; i < this.points.length; i++) {
            if (this.points[i] !== other.points[i]) return false;
        }
        return true;
    }
    transformed(f) {
        const newCubic = new MutableCubic();
        newCubic.points.set(this.points);
        newCubic.transform(f);
        return new Cubic(newCubic.points);
    }
    static straightLine(x0, y0, x1, y1) {
        return createCubic(
            x0, y0,
            interpolate(x0, x1, 1 / 3), interpolate(y0, y1, 1 / 3),
            interpolate(x0, x1, 2 / 3), interpolate(y0, y1, 2 / 3),
            x1, y1
        );
    }
    static circularArc(centerX, centerY, x0, y0, x1, y1) {
        const p0d = directionVector(x0 - centerX, y0 - centerY);
        const p1d = directionVector(x1 - centerX, y1 - centerY);
        const rotatedP0 = p0d.rotate90();
        const rotatedP1 = p1d.rotate90();
        const clockwise = rotatedP0.dotProduct(x1 - centerX, y1 - centerY) >= 0;
        const cosa = p0d.dotProduct(p1d);
        if (cosa > 0.999) return Cubic.straightLine(x0, y0, x1, y1);
        const k = distance(x0 - centerX, y0 - centerY) * 4 / 3 *
                  (Math.sqrt(2 * (1 - cosa)) - Math.sqrt(1 - cosa * cosa)) / (1 - cosa) *
                  (clockwise ? 1 : -1);
        return createCubic(
            x0, y0,
            x0 + rotatedP0.x * k, y0 + rotatedP0.y * k,
            x1 - rotatedP1.x * k, y1 - rotatedP1.y * k,
            x1, y1
        );
    }
    static empty(x0, y0) {
        return createCubic(x0, y0, x0, y0, x0, y0, x0, y0);
    }
}
function createCubic(
    anchor0X, anchor0Y, control0X, control0Y,
    control1X, control1Y, anchor1X, anchor1Y
) {
    return new Cubic(new Float32Array([
        anchor0X, anchor0Y, control0X, control0Y,
        control1X, control1Y, anchor1X, anchor1Y
    ]));
}
class MutableCubic extends Cubic {
    transformOnePoint(f, ix) {
        const result = f.transform(this.points[ix], this.points[ix + 1]);
        this.points[ix] = result.first;
        this.points[ix + 1] = result.second;
    }
    transform(f) {
        this.transformOnePoint(f, 0);
        this.transformOnePoint(f, 2);
        this.transformOnePoint(f, 4);
        this.transformOnePoint(f, 6);
    }
    interpolate(c1, c2, progress) {
        for (let i = 0; i < 8; i++) {
            this.points[i] = interpolate(c1.points[i], c2.points[i], progress);
        }
    }
}
function progressInRange(progress, progressFrom, progressTo) {
    if (progressTo >= progressFrom) {
        return progress >= progressFrom && progress <= progressTo;
    } else {
        return progress >= progressFrom || progress <= progressTo;
    }
}
function linearMap(xValues, yValues, x) {
    if (isNaN(x) || !xValues || !yValues || xValues.length === 0 || yValues.length === 0) {
        console.error(`❌ linearMap: Invalid input - x=${x}, xValues=${xValues}, yValues=${yValues}`);
        return 0; // Return safe default
    }
    if (xValues.some(isNaN) || yValues.some(isNaN)) {
        console.error(`❌ linearMap: NaN values in arrays - xValues=${xValues}, yValues=${yValues}`);
        return 0; // Return safe default
    }
    if (x < 0 || x > 1) {
        if (x < -DistanceEpsilon || x > 1 + DistanceEpsilon) {
            throw new Error(`Invalid progress: ${x}`);
        }
        x = Math.max(0, Math.min(1, x));
    }
    let segmentStartIndex = -1;
    for (let i = 0; i < xValues.length; i++) {
        if (progressInRange(x, xValues[i], xValues[(i + 1) % xValues.length])) {
            segmentStartIndex = i;
            break;
        }
    }
    if (segmentStartIndex === -1) {
        let minDist = Infinity;
        for (let i = 0; i < xValues.length; i++) {
            const dist = progressDistance(x, xValues[i]);
            if (dist < minDist) {
                minDist = dist;
                segmentStartIndex = i;
            }
        }
    }
    const segmentEndIndex = (segmentStartIndex + 1) % xValues.length;
    const segmentSizeX = positiveModulo(xValues[segmentEndIndex] - xValues[segmentStartIndex], 1);
    const segmentSizeY = positiveModulo(yValues[segmentEndIndex] - yValues[segmentStartIndex], 1);
    const positionInSegment = (segmentSizeX < 0.001) ?
        0.5 :
        positiveModulo(x - xValues[segmentStartIndex], 1) / segmentSizeX;
    return positiveModulo(yValues[segmentStartIndex] + segmentSizeY * positionInSegment, 1);
}
class DoubleMapper {
    #sourceValues;
    #targetValues;
    constructor(...mappings) {
        this.#sourceValues = new Array(mappings.length);
        this.#targetValues = new Array(mappings.length);
        for (let i = 0; i < mappings.length; i++) {
            this.#sourceValues[i] = mappings[i].first;
            this.#targetValues[i] = mappings[i].second;
        }
        validateProgress(this.#sourceValues);
        validateProgress(this.#targetValues);
    }
    map(x) {
        return linearMap(this.#sourceValues, this.#targetValues, x);
    }
    mapBack(x) {
        return linearMap(this.#targetValues, this.#sourceValues, x);
    }
    static Identity = new DoubleMapper({
        first: 0,
        second: 0
    }, {
        first: 0.5,
        second: 0.5
    }, );
}
function validateProgress(p) {
    if (p.length === 0) return;
    let prev = p[p.length - 1];
    let wraps = 0;
    for (let i = 0; i < p.length; i++) {
        const curr = p[i];
        if (curr < 0 || curr >= 1) {
            throw new Error(`FloatMapping - Progress outside of range: ${p.join(', ')}`);
        }
        if (progressDistance(curr, prev) <= DistanceEpsilon) {
            throw new Error(`FloatMapping - Progress repeats a value: ${p.join(', ')}`);
        }
        if (curr < prev) {
            wraps++;
            if (wraps > 1) {
                throw new Error(`FloatMapping - Progress wraps more than once: ${p.join(', ')}`);
            }
        }
        prev = curr;
    }
}
function progressDistance(p1, p2) {
    const d = Math.abs(p1 - p2);
    return Math.min(d, 1 - d);
}
const LOG_TAG = "FeatureMapping";
class ProgressableFeature {
    constructor(progress, feature) {
        this.progress = progress;
        this.feature = feature;
    }
}
class DistanceVertex {
    constructor(distance, f1, f2) {
        this.distance = distance;
        this.f1 = f1;
        this.f2 = f2;
    }
}
function featureMapper(features1, features2) {
    const filteredFeatures1 = [];
    for (const f of features1) {
        if (f.feature.isCorner) {
            filteredFeatures1.push(f);
        }
    }
    const filteredFeatures2 = [];
    for (const f of features2) {
        if (f.feature.isCorner) {
            filteredFeatures2.push(f);
        }
    }
    const featureProgressMapping = doMapping(filteredFeatures1, filteredFeatures2);
    if (DEBUG) {
        debugLog(LOG_TAG, featureProgressMapping.map(p => `${p.first} -> ${p.second}`).join(', '));
    }
    const dm = new DoubleMapper(...featureProgressMapping);
    if (DEBUG) {
        const N = 10;
        const toFixed = (n) => n.toFixed(3);
        const mapValues = Array.from({ length: N + 1 }, (_, i) => toFixed(dm.map(i / N))).join(', ');
        const mapBackValues = Array.from({ length: N + 1 }, (_, i) => toFixed(dm.mapBack(i / N))).join(', ');
        debugLog(LOG_TAG, `Map: ${mapValues}\nMb : ${mapBackValues}`);
    }
    return dm;
}
function doMapping(features1, features2) {
    if (DEBUG) {
        debugLog(LOG_TAG, `Shape1 progresses: ${features1.map(f => f.progress).join(', ')}`);
        debugLog(LOG_TAG, `Shape2 progresses: ${features2.map(f => f.progress).join(', ')}`);
    }
    const distanceVertexList = [];
    for (const f1 of features1) {
        for (const f2 of features2) {
            const d = featureDistSquared(f1.feature, f2.feature);
            if (d !== Number.MAX_VALUE) {
                distanceVertexList.push(new DistanceVertex(d, f1, f2));
            }
        }
    }
    distanceVertexList.sort((a, b) => a.distance - b.distance);
    if (distanceVertexList.length === 0) return IdentityMapping;
    if (distanceVertexList.length === 1) {
        const { f1, f2 } = distanceVertexList[0];
        const p1 = f1.progress;
        const p2 = f2.progress;
        return [
            { first: p1, second: p2 },
            { first: (p1 + 0.5) % 1, second: (p2 + 0.5) % 1 }
        ];
    }
    const helper = new MappingHelper();
    distanceVertexList.forEach(vertex => helper.addMapping(vertex.f1, vertex.f2));
    return helper.mapping;
}
const IdentityMapping = [{ first: 0, second: 0 }, { first: 0.5, second: 0.5 }];
function binarySearchBy(sortedArray, key, selector) {
    let low = 0;
    let high = sortedArray.length - 1;
    while (low <= high) {
        const mid = Math.floor((low + high) / 2);
        const midVal = selector(sortedArray[mid]);
        if (midVal < key) low = mid + 1;
        else if (midVal > key) high = mid - 1;
        else return mid;
    }
    return -(low + 1);
}
function progressDistance(p1, p2) {
    const d = Math.abs(p1 - p2);
    return Math.min(d, 1 - d);
}
function progressInRange(p, start, end) {
    return start <= end ? p >= start && p <= end : p >= start || p <= end;
}
class MappingHelper {
    constructor() {
        this.mapping = []; // {first: number, second: number}[]
        this.usedF1 = new Set(); // Set<ProgressableFeature>
        this.usedF2 = new Set(); // Set<ProgressableFeature>
    }
    addMapping(f1, f2) {
        if (this.usedF1.has(f1) || this.usedF2.has(f2)) return;
        const index = binarySearchBy(this.mapping, f1.progress, item => item.first);
        if (index >= 0) {
            return;
        }
        const insertionIndex = -index - 1;
        const n = this.mapping.length;
        if (n >= 1) {
            const before = this.mapping[(insertionIndex + n - 1) % n];
            const after = this.mapping[insertionIndex % n];
            if (
                progressDistance(f1.progress, before.first) < DistanceEpsilon ||
                progressDistance(f1.progress, after.first) < DistanceEpsilon ||
                progressDistance(f2.progress, before.second) < DistanceEpsilon ||
                progressDistance(f2.progress, after.second) < DistanceEpsilon
            ) {
                return;
            }
            if (n > 1 && !progressInRange(f2.progress, before.second, after.second)) {
                return;
            }
        }
        this.mapping.splice(insertionIndex, 0, { first: f1.progress, second: f2.progress });
        this.usedF1.add(f1);
        this.usedF2.add(f2);
    }
}
function featureDistSquared(f1, f2) {
    if (f1.isCorner && f2.isCorner && f1.convex !== f2.convex) {
        if (DEBUG) debugLog(LOG_TAG, "*** Feature distance ∞ for convex-vs-concave corners");
        return Number.MAX_VALUE;
    }
    const p1 = featureRepresentativePoint(f1);
    const p2 = featureRepresentativePoint(f2);
    const dx = p1.x - p2.x;
    const dy = p1.y - p2.y;
    return dx * dx + dy * dy;
}
function featureRepresentativePoint(feature) {
    const firstCubic = feature.cubics[0];
    const lastCubic = feature.cubics[feature.cubics.length - 1];
    const x = (firstCubic.anchor0X + lastCubic.anchor1X) / 2;
    const y = (firstCubic.anchor0Y + lastCubic.anchor1Y) / 2;
    return new Point(x, y);
}
class MeasuredCubic {
    cubic;
    startOutlineProgress;
    endOutlineProgress;
    #measurer;
    measuredSize;
    constructor(cubic, startOutlineProgress, endOutlineProgress, measurer) {
        if (endOutlineProgress < startOutlineProgress) {
            if (endOutlineProgress < startOutlineProgress - DistanceEpsilon) {
                throw new Error(
                   `endOutlineProgress (${endOutlineProgress}) is expected to be equal or ` +
                   `greater than startOutlineProgress (${startOutlineProgress})`
                );
            }
            endOutlineProgress = startOutlineProgress;
        }
        this.cubic = cubic;
        this.startOutlineProgress = startOutlineProgress;
        this.endOutlineProgress = endOutlineProgress;
        this.#measurer = measurer;
        this.measuredSize = this.#measurer.measureCubic(cubic);
    }
    updateProgressRange(
        startOutlineProgress = this.startOutlineProgress,
        endOutlineProgress = this.endOutlineProgress
    ) {
        if (endOutlineProgress < startOutlineProgress) {
            throw new Error("endOutlineProgress is expected to be equal or greater than startOutlineProgress");
        }
        this.startOutlineProgress = startOutlineProgress;
        this.endOutlineProgress = endOutlineProgress;
    }
    cutAtProgress(cutOutlineProgress) {
        const boundedCutOutlineProgress = Math.max(
            this.startOutlineProgress,
            Math.min(cutOutlineProgress, this.endOutlineProgress)
        );
        const outlineProgressSize = this.endOutlineProgress - this.startOutlineProgress;
        const progressFromStart = boundedCutOutlineProgress - this.startOutlineProgress;
        const relativeProgress = outlineProgressSize === 0 ? 0 : progressFromStart / outlineProgressSize;
        const t = this.#measurer.findCubicCutPoint(this.cubic, relativeProgress * this.measuredSize);
        if (t < 0 || t > 1) {
            if (t < -DistanceEpsilon || t > 1 + DistanceEpsilon) {
                throw new Error(`Cubic cut point ${t} is expected to be between 0 and 1`);
            }
        }
        if (DEBUG) {
            debugLog(LOG_TAG,
                `cutAtProgress: progress = ${boundedCutOutlineProgress} / ` +
                `this = [${this.startOutlineProgress} .. ${this.endOutlineProgress}] / ` +
                `ps = ${progressFromStart} / rp = ${relativeProgress} / t = ${t}`
            );
        }
        const [c1, c2] = this.cubic.split(t);
        return [
            new MeasuredCubic(c1, this.startOutlineProgress, boundedCutOutlineProgress, this.#measurer),
            new MeasuredCubic(c2, boundedCutOutlineProgress, this.endOutlineProgress, this.#measurer)
        ];
    }
    toString() {
        return `MeasuredCubic(outlineProgress=[${this.startOutlineProgress} .. ${this.endOutlineProgress}], ` +
               `size=${this.measuredSize}, cubic=${this.cubic})`;
    }
}
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
