pub(crate) fn get_js() -> &'static str {
    r#"
function select(idx) {
    window.ipc.postMessage('select:' + idx);
}

function dismiss() {
    window.ipc.postMessage('dismiss');
}

const grid = document.getElementById('grid');
let items = [];
const dismissBtn = document.querySelector('.dismiss-btn');

const MAX_SCALE = 1.10;
const MIN_SCALE = 1.0;
const EFFECT_RADIUS = 80;
const BASE_WEIGHT = 500;
const MAX_WEIGHT = 650;
const BASE_WIDTH = 100;
const MAX_WIDTH = 104;

let animationFrame = null;
let mouseX = -1000;
let mouseY = -1000;
let isMouseInGrid = false;
let itemCenters = new Map();

function cacheItemPositions() {
    items.forEach(item => {
        item.style.transform = 'scale(1)';
    });

    itemCenters.clear();
    items.forEach(item => {
        const rect = item.getBoundingClientRect();
        itemCenters.set(item, {
            x: rect.left + rect.width / 2,
            y: rect.top + rect.height / 2
        });
    });
}

function getItemCenter(item) {
    const cached = itemCenters.get(item);
    if (cached) return cached;

    const rect = item.getBoundingClientRect();
    return {
        x: rect.left + rect.width / 2,
        y: rect.top + rect.height / 2
    };
}

function isMouseInRect(rect) {
    return mouseX >= rect.left && mouseX <= rect.right &&
           mouseY >= rect.top && mouseY <= rect.bottom;
}

function updateFisheye() {
    items.forEach(item => {
        if (!item.classList.contains('visible')) return;

        const center = getItemCenter(item);
        const dx = mouseX - center.x;
        const dy = mouseY - center.y;
        const distance = Math.sqrt(dx * dx + dy * dy);

        let influence = isMouseInGrid ? Math.max(0, 1 - distance / EFFECT_RADIUS) : 0;
        influence = influence * influence * (3 - 2 * influence);

        const scale = MIN_SCALE + (MAX_SCALE - MIN_SCALE) * influence;
        const rect = item.getBoundingClientRect();
        const isHovered = isMouseInGrid && isMouseInRect(rect);

        if (isHovered) {
            item.classList.add('hovered');
            item.style.fontVariationSettings = '';
            item.style.letterSpacing = '';
        } else {
            item.classList.remove('hovered');
            const weight = BASE_WEIGHT + (MAX_WEIGHT - BASE_WEIGHT) * influence;
            const width = BASE_WIDTH + (MAX_WIDTH - BASE_WIDTH) * influence;
            item.style.fontVariationSettings = `'wght' ${weight.toFixed(0)}, 'wdth' ${width.toFixed(0)}, 'ROND' 100`;
            item.style.letterSpacing = '0';
        }

        item.style.transform = `scale(${scale.toFixed(3)})`;
    });
}

function onMouseMove(e) {
    mouseX = e.clientX;
    mouseY = e.clientY;

    if (!animationFrame) {
        animationFrame = requestAnimationFrame(() => {
            updateFisheye();
            animationFrame = null;
        });
    }
}

function onMouseEnter() {
    isMouseInGrid = true;
}

function onMouseLeave() {
    isMouseInGrid = false;
    mouseX = -1000;
    mouseY = -1000;

    items.forEach(item => {
        item.style.transform = 'scale(1)';
        item.style.fontVariationSettings = `'wght' ${BASE_WEIGHT}, 'wdth' ${BASE_WIDTH}, 'ROND' 100`;
        item.classList.remove('hovered');
    });
}

grid.addEventListener('mousemove', onMouseMove);
grid.addEventListener('mouseenter', onMouseEnter);
grid.addEventListener('mouseleave', onMouseLeave);

document.querySelector('.container').addEventListener('mousemove', (e) => {
    const gridRect = grid.getBoundingClientRect();
    const padding = 35;
    if (e.clientX >= gridRect.left - padding &&
        e.clientX <= gridRect.right + padding &&
        e.clientY >= gridRect.top - padding &&
        e.clientY <= gridRect.bottom + padding) {
        onMouseMove(e);
    }
});

function animateIn() {
    const windowCenterX = window.innerWidth / 2;
    const windowCenterY = window.innerHeight / 2;

    const itemsWithDistance = items.map(item => {
        const rect = item.getBoundingClientRect();
        const itemCenterX = rect.left + rect.width / 2;
        const itemCenterY = rect.top + rect.height / 2;
        const dx = itemCenterX - windowCenterX;
        const dy = itemCenterY - windowCenterY;
        const distance = Math.sqrt(dx * dx + dy * dy);
        return { item, distance };
    });

    itemsWithDistance.sort((a, b) => a.distance - b.distance);

    setTimeout(() => dismissBtn.classList.add('visible'), 0);

    itemsWithDistance.forEach(({ item }, i) => {
        setTimeout(() => item.classList.add('visible'), i * 12);
    });

    const totalAnimationTime = itemsWithDistance.length * 12 + 150;
    setTimeout(() => cacheItemPositions(), totalAnimationTime);
}

window.updateContent = function(itemsHtml, dismissLabel) {
    grid.innerHTML = itemsHtml;
    dismissBtn.innerText = dismissLabel;
    items = Array.from(document.querySelectorAll('.preset-item'));
    itemCenters.clear();

    dismissBtn.classList.remove('visible');
    items.forEach(item => item.classList.remove('visible'));

    setTimeout(() => {
        window.ipc.postMessage('ready_to_show');
        setTimeout(() => requestAnimationFrame(animateIn), 16);

        setTimeout(() => {
            dismissBtn.classList.add('visible');
            items.forEach(item => item.classList.add('visible'));
        }, 300);
    }, 0);
};

document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') dismiss();
});
    "#
}
