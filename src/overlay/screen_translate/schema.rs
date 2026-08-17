//! Provider-neutral translation-only response shape.

pub(super) fn response_schema(cell_count: usize, max_text_chars: usize) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "cells": {
                "type": "array",
                "minItems": 1,
                "maxItems": cell_count,
                "items": {
                    "type": "object",
                    "properties": {
                        "cellId": { "type": "integer" },
                        "translation": {
                            "type": "string",
                            "minLength": 1,
                            "maxLength": max_text_chars
                        },
                        "splitAfterMembers": {
                            "type": "array",
                            "maxItems": 3,
                            "items": { "type": "integer" }
                        }
                    },
                    "required": ["cellId", "translation", "splitAfterMembers"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["cells"],
        "additionalProperties": false
    })
}
