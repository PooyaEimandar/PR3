// A Stop click belongs to one turn, including the time before its start event arrives.
// Finishing invalidates delayed events so they cannot stop a subsequent turn.
export function turnStop(requested: () => void, stop: (id: number) => void) {
  let active = true;
  let wanted = false;
  let sent = false;
  let task: number | null = null;
  const dispatch = () => {
    if (active && wanted && !sent && task !== null) {
      sent = true;
      stop(task);
    }
  };
  return {
    get active(): boolean { return active; },
    get requested(): boolean { return wanted; },
    request(): void {
      if (!active || wanted) return;
      wanted = true;
      requested();
      dispatch();
    },
    started(id: number): void {
      if (!active || task !== null) return;
      task = id;
      dispatch();
    },
    failed(): void {
      if (active) { wanted = false; sent = false; }
    },
    finish(): void { active = false; },
  };
}
