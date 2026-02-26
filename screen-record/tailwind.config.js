/** @type {import('tailwindcss').Config} */
module.exports = {
    darkMode: ["class"],
    content: ["./index.html", "./src/**/*.{ts,tsx,js,jsx}"],
  theme: {
  	extend: {
  		borderRadius: {
  			lg: 'var(--radius)',
  			md: 'calc(var(--radius) - 2px)',
  			sm: 'calc(var(--radius) - 4px)'
  		},
  		colors: {
  			// shadcn/ui semantic colors
  			background: 'hsl(var(--background))',
  			foreground: 'hsl(var(--foreground))',
  			card: {
  				DEFAULT: 'hsl(var(--card))',
  				foreground: 'hsl(var(--card-foreground))'
  			},
  			popover: {
  				DEFAULT: 'hsl(var(--popover))',
  				foreground: 'hsl(var(--popover-foreground))'
  			},
  			primary: {
  				DEFAULT: 'hsl(var(--primary))',
  				foreground: 'hsl(var(--primary-foreground))'
  			},
  			secondary: {
  				DEFAULT: 'hsl(var(--secondary))',
  				foreground: 'hsl(var(--secondary-foreground))'
  			},
  			muted: {
  				DEFAULT: 'hsl(var(--muted))',
  				foreground: 'hsl(var(--muted-foreground))'
  			},
  			accent: {
  				DEFAULT: 'hsl(var(--accent))',
  				foreground: 'hsl(var(--accent-foreground))'
  			},
  			destructive: {
  				DEFAULT: 'hsl(var(--destructive))',
  				foreground: 'hsl(var(--destructive-foreground))'
  			},
  			border: 'hsl(var(--border))',
  			input: 'hsl(var(--input))',
  			ring: 'hsl(var(--ring))',
  			chart: {
  				'1': 'hsl(var(--chart-1))',
  				'2': 'hsl(var(--chart-2))',
  				'3': 'hsl(var(--chart-3))',
  				'4': 'hsl(var(--chart-4))',
  				'5': 'hsl(var(--chart-5))'
  			},
  			// Project design tokens — mapped from App.css CSS variables.
  			// Note: opacity modifiers (bg-surface/50) won't work with these since
  			// they use raw CSS var references, not the HSL channel pattern.
  			surface: {
  				DEFAULT: 'var(--surface)',
  				dim: 'var(--surface-dim)',
  				container: 'var(--surface-container)',
  				'container-high': 'var(--surface-container-high)',
  				'container-highest': 'var(--surface-container-highest)',
  			},
  			'on-surface': {
  				DEFAULT: 'var(--on-surface)',
  				variant: 'var(--on-surface-variant)',
  			},
  			glass: {
  				bg: 'var(--glass-bg)',
  				border: 'var(--glass-border)',
  				hover: 'var(--glass-bg-hover)',
  			},
  			'primary-color': 'var(--primary-color)',
  			'tertiary-color': 'var(--tertiary-color)',
  			'outline-variant': 'var(--outline-variant)',
  		}
  	}
  },
  plugins: [require("tailwindcss-animate")],
}
