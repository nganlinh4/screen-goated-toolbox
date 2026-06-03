
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
