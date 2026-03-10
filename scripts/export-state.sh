#!/usr/bin/env bash

set -euo pipefail

usage() {
    echo "Usage: $0 <contract-address[,contract-address...]> <height>"
    exit 1
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Error: required command '$1' is not installed or not in PATH" >&2
        exit 1
    fi
}

sanitize_label() {
    local value="$1"
    value="$(echo "$value" | tr '[:upper:]' '[:lower:]')"
    value="$(echo "$value" | sed -E 's/[^a-z0-9._-]+/-/g; s/^-+//; s/-+$//')"
    if [[ -z "$value" ]]; then
        value="unknown-contract"
    fi
    echo "$value"
}

if [[ $# -ne 2 ]]; then
    usage
fi

ADDRESSES_CSV="$1"
HEIGHT="$2"
LIMIT="${EXPORT_STATE_PAGE_LIMIT:-1000}"

if ! [[ "$HEIGHT" =~ ^[0-9]+$ ]]; then
    echo "Error: height must be a positive integer, got '$HEIGHT'" >&2
    exit 1
fi

require_cmd thornode
require_cmd jq

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
STATE_DIR="$ROOT_DIR/packages/rujira-rs-testing/src/mock/state/$HEIGHT"
WASM_OUT_DIR="$STATE_DIR/wasm"
BANK_OUT_DIR="$STATE_DIR/bank"
mkdir -p "$WASM_OUT_DIR" "$BANK_OUT_DIR"

TMP_OUTPUT="$(mktemp)"
TMP_PAGE="$(mktemp)"
cleanup() {
    rm -f "$TMP_OUTPUT" "$TMP_PAGE"
}
trap cleanup EXIT

export_for_address() {
    local address="$1"
    local contract_info_json
    local address_label
    local contract_label
    local wasm_output_path
    local bank_output_path
    local page_key
    local page
    local merged_tmp
    local page_count
    local total_count
    local cmd
    
    contract_info_json="$(thornode q wasm contract "$address" -o json 2>/dev/null || true)"
    address_label="$address"
    if [[ -n "$contract_info_json" ]]; then
        contract_label="$(echo "$contract_info_json" | jq -r '.contract_info.label // .label // empty')"
        if [[ -n "$contract_label" ]]; then
            address_label="$(sanitize_label "$contract_label")"
        fi
    fi
    
    wasm_output_path="$WASM_OUT_DIR/$address_label.json"
    bank_output_path="$BANK_OUT_DIR/$address_label.json"
    
    echo "Exporting address: $address (label: $address_label)"
    
    echo '{"models":[]}' > "$TMP_OUTPUT"
    page_key=""
    page=1
    
    while true; do
        cmd=(thornode q wasm contract-state all "$address" --height "$HEIGHT" --limit "$LIMIT" -o json)
        if [[ -n "$page_key" ]]; then
            cmd+=(--page-key "$page_key")
        fi
        
        "${cmd[@]}" > "$TMP_PAGE"
        
        if ! jq -e '.models | type == "array"' "$TMP_PAGE" >/dev/null 2>&1; then
            echo "Error: unexpected response shape while fetching wasm page $page for $address" >&2
            cat "$TMP_PAGE" >&2
            exit 1
        fi
        
        merged_tmp="$(mktemp)"
        jq -n \
        --argfile acc "$TMP_OUTPUT" \
        --argfile page "$TMP_PAGE" \
        '{models: ($acc.models + $page.models)}' > "$merged_tmp"
        mv "$merged_tmp" "$TMP_OUTPUT"
        
        page_count="$(jq '.models | length' "$TMP_PAGE")"
        echo "Fetched wasm page $page for $address ($page_count entries)"
        
        page_key="$(jq -r '.pagination.next_key // empty' "$TMP_PAGE")"
        if [[ -z "$page_key" ]]; then
            break
        fi
        
        page=$((page + 1))
    done
    
    mv "$TMP_OUTPUT" "$wasm_output_path"
    total_count="$(jq '.models | length' "$wasm_output_path")"
    echo "Exported wasm state: $total_count entries to $wasm_output_path"
    
    echo '{"balances":[]}' > "$TMP_OUTPUT"
    page_key=""
    page=1
    
    while true; do
        cmd=(thornode q bank balances "$address" --height "$HEIGHT" -o json)
        if [[ -n "$page_key" ]]; then
            cmd+=(--page-key "$page_key")
        fi
        
        "${cmd[@]}" > "$TMP_PAGE"
        
        if ! jq -e '.balances | type == "array"' "$TMP_PAGE" >/dev/null 2>&1; then
            echo "Error: unexpected response shape while fetching bank page $page for $address" >&2
            cat "$TMP_PAGE" >&2
            exit 1
        fi
        
        merged_tmp="$(mktemp)"
        jq -n \
        --argfile acc "$TMP_OUTPUT" \
        --argfile page "$TMP_PAGE" \
        '{balances: ($acc.balances + $page.balances)}' > "$merged_tmp"
        mv "$merged_tmp" "$TMP_OUTPUT"
        
        page_count="$(jq '.balances | length' "$TMP_PAGE")"
        echo "Fetched bank page $page for $address ($page_count entries)"
        
        page_key="$(jq -r '.pagination.next_key // empty' "$TMP_PAGE")"
        if [[ -z "$page_key" ]]; then
            break
        fi
        
        page=$((page + 1))
    done
    
    mv "$TMP_OUTPUT" "$bank_output_path"
    total_count="$(jq '.balances | length' "$bank_output_path")"
    echo "Exported bank balances: $total_count entries to $bank_output_path"
}

IFS=',' read -r -a ADDRESSES <<< "$ADDRESSES_CSV"

if [[ ${#ADDRESSES[@]} -eq 0 ]]; then
    echo "Error: no addresses provided" >&2
    exit 1
fi

exported_count=0
for raw_address in "${ADDRESSES[@]}"; do
    address="$(echo "$raw_address" | sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//')"
    if [[ -z "$address" ]]; then
        continue
    fi
    export_for_address "$address"
    exported_count=$((exported_count + 1))
done

if [[ $exported_count -eq 0 ]]; then
    echo "Error: no non-empty addresses provided" >&2
    exit 1
fi
