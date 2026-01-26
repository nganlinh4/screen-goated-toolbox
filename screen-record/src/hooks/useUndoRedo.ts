import { useState, useCallback, useRef } from 'react';

// A simple hook to manage undo/redo history
export function useUndoRedo<T>(initialState: T, maxHistory: number = 20) {
    const [present, setPresent] = useState<T>(initialState);
    const [past, setPast] = useState<T[]>([]);
    const [future, setFuture] = useState<T[]>([]);

    // We need refs to access latest state in callbacks without re-creating them
    const presentRef = useRef(present);
    presentRef.current = present;

    const set = useCallback((newState: T | ((prev: T) => T), withHistory: boolean = true) => {
        // If newState is a function, evaluate it
        const computedState = newState instanceof Function ? newState(presentRef.current) : newState;

        // Don't save if state hasn't changed (deep comparison/shallow expected?)
        // Basic strict equality for now.
        if (computedState === presentRef.current) return;

        if (withHistory) {
            setPast((prev) => {
                const newPast = [...prev, presentRef.current];
                if (newPast.length > maxHistory) newPast.shift();
                return newPast;
            });
            setFuture([]);
        }
        setPresent(computedState);
    }, [maxHistory]);

    const undo = useCallback(() => {
        if (past.length === 0) return;

        const previous = past[past.length - 1];
        const newPast = past.slice(0, past.length - 1);

        setFuture((prev) => [presentRef.current, ...prev]);
        setPresent(previous);
        setPast(newPast);
    }, [past]);

    const redo = useCallback(() => {
        if (future.length === 0) return;

        const next = future[0];
        const newFuture = future.slice(1);

        setPast((prev) => [...prev, presentRef.current]);
        setPresent(next);
        setFuture(newFuture);
    }, [future]);

    return {
        state: present,
        setState: set,
        undo,
        redo,
        canUndo: past.length > 0,
        canRedo: future.length > 0
    };
}
