type WindowWithInvoke = Window & {
  invoke?: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
};

export const invoke = <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
  const w = window as WindowWithInvoke;
  if (typeof w.invoke === 'function') {
    return w.invoke<T>(cmd, args);
  }
  return Promise.reject(new Error('IPC not available'));
};
