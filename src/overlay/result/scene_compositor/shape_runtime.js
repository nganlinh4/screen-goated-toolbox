(function() {
    function graphemeUnits(text) {
        if (typeof Intl !== 'undefined' && Intl.Segmenter) {
            var segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
            return Array.from(segmenter.segment(text), function(part) { return part.segment; });
        }
        return Array.from(text);
    }

    function prefersVerticalWriting(text) {
        var visible = text.match(/[\p{L}\p{N}]/gu) || [];
        if (!visible.length) return false;
        var vertical = text.match(/[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}]/gu) || [];
        return vertical.length * 2 >= visible.length;
    }

    function textUnits(text, orientation, context, maximumCapacity) {
        if (orientation === 'vertical') return graphemeUnits(text);
        var words = text.match(/\S+\s*|\s+/gu) || [];
        var units = [];
        for (var index = 0; index < words.length; index++) {
            var word = words[index];
            if (context.measureText(word).width <= maximumCapacity) units.push(word);
            else units.push.apply(units, graphemeUnits(word));
        }
        return units;
    }

    function maskRuns(data, imageWidth, imageHeight, orientation, offset, thickness) {
        var extent = orientation === 'horizontal' ? imageWidth : imageHeight;
        var depth = orientation === 'horizontal' ? imageHeight : imageWidth;
        var from = Math.max(0, Math.floor(offset));
        var to = Math.min(depth, Math.ceil(offset + thickness));
        var runs = [];
        var start = -1;
        for (var position = 0; position < extent; position++) {
            var opaque = 0;
            var samples = 0;
            for (var cross = from; cross < to; cross++) {
                var x = orientation === 'horizontal' ? position : cross;
                var y = orientation === 'horizontal' ? cross : position;
                samples++;
                if (data[(y * imageWidth + x) * 4 + 3] >= 192) opaque++;
            }
            var legal = samples > 0 && opaque === samples;
            if (legal && start < 0) start = position;
            if ((!legal || position === extent - 1) && start >= 0) {
                var end = legal && position === extent - 1 ? position + 1 : position;
                if (end - start >= 2) runs.push([start, end]);
                start = -1;
            }
        }
        return runs;
    }

    function shapeSlots(alpha, width, height, orientation, lineSize) {
        var slots = [];
        var depth = orientation === 'horizontal' ? height : width;
        var offsets = [];
        for (var offset = 0; offset + lineSize <= depth + 0.5; offset += lineSize) {
            offsets.push(offset);
        }
        if (orientation === 'vertical') offsets.reverse();
        for (var index = 0; index < offsets.length; index++) {
            var runs = maskRuns(alpha, width, height, orientation, offsets[index], lineSize);
            for (var runIndex = 0; runIndex < runs.length; runIndex++) {
                slots.push({ offset: offsets[index], from: runs[runIndex][0], to: runs[runIndex][1] });
            }
        }
        return slots;
    }

    function fillPlan(text, alpha, width, height, orientation, fontSize, stretch) {
        var context = document.createElement('canvas').getContext('2d');
        context.font = '400 ' + fontSize + "px 'Google Sans Flex'";
        if ('fontStretch' in context) context.fontStretch = stretch + '%';
        var lineSize = Math.max(1, fontSize * 1.3);
        var slots = shapeSlots(alpha, width, height, orientation, lineSize);
        if (!slots.length) return null;
        var cursor = 0;
        var maximumCapacity = slots.reduce(function(maximum, slot) {
            return Math.max(maximum, slot.to - slot.from);
        }, 0);
        var units = textUnits(text.replace(/\s+/g, ' ').trim(), orientation, context, maximumCapacity);
        var lines = [];
        for (var slotIndex = 0; slotIndex < slots.length && cursor < units.length; slotIndex++) {
            var slot = slots[slotIndex];
            var capacity = slot.to - slot.from;
            var value = '';
            while (cursor < units.length) {
                var unit = units[cursor];
                var candidate = value + unit;
                var advance = context.measureText(candidate).width;
                if (value && advance > capacity) break;
                if (!value && advance > capacity) break;
                value += unit;
                cursor++;
            }
            value = value.trim();
            if (value) lines.push({ slot: slot, text: value });
        }
        return {
            orientation: orientation,
            fontSize: fontSize,
            lineSize: lineSize,
            stretch: stretch,
            lines: lines,
            consumed: cursor,
            complete: cursor >= units.length
        };
    }

    window.__SGT_SHAPE_LAYOUT__ = {
        fillPlan: fillPlan,
        prefersVerticalWriting: prefersVerticalWriting
    };
})();
