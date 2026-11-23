use super::*;

pub(super) fn selection_expression(expected: TargetExpectation<'_>) -> String {
    format!(
        r#"(() => {{ const documentId=({document_id}); const e=document.querySelector({selector});
            const elementId=e && (globalThis[Symbol.for('sgt.controller.private-state')].nodes.get(e) || null);
            if(documentId !== {expected_document} || !e || elementId !== {expected_element} ||
                e.disabled || e.readOnly || !(e===document.activeElement || e.contains(document.activeElement)))
                return {{selectionReady:false}};
            if(e instanceof HTMLInputElement || e instanceof HTMLTextAreaElement)
                return {{selectionReady:e.selectionStart===0 && e.selectionEnd===e.value.length}};
            if(!e.isContentEditable || e.parentElement?.isContentEditable) return {{selectionReady:false}};
            const selection=document.getSelection();
            if(!selection || selection.rangeCount!==1) return {{selectionReady:false}};
            const range=selection.getRangeAt(0);
            return {{selectionReady:range.startContainer===e && range.startOffset===0 &&
                range.endContainer===e && range.endOffset===e.childNodes.length}}; }})()"#,
        selector = json!(expected.selector),
        expected_document = json!(expected.document_id),
        expected_element = json!(expected.element_id),
        document_id = super::super::controller_isolated::DOCUMENT_ID_JS,
    )
}
