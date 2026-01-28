
export interface SpringConfig {
    stiffness: number;
    damping: number;
    mass: number;
}

export class SpringSolver {
    private x: number;
    private v: number;
    private config: SpringConfig;

    constructor(initialValue: number, config: SpringConfig) {
        this.x = initialValue;
        this.v = 0;
        this.config = config;
    }

    public update(target: number, dt: number): number {
        // Basic spring physics integration (Semi-implicit Euler)
        // Force = -k * (x - target) - damping * v
        const force = -this.config.stiffness * (this.x - target) - this.config.damping * this.v;
        const acceleration = force / this.config.mass;

        this.v += acceleration * dt;
        this.x += this.v * dt;

        return this.x;
    }

    public set(value: number) {
        this.x = value;
        this.v = 0;
    }
}
