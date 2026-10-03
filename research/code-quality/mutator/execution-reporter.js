// Vitest's JSON report omits unhandled errors; retain them independently.
export default class ExecutionReporter {
  onTestRunEnd(modules, unhandledErrors, reason) {
    const suiteErrors = [];
    function visit(task) {
      if (task.type !== 'test' && task.result?.errors?.length) {
        suiteErrors.push(...task.result.errors.map(error => ({ name: error.name, message: error.message })));
      }
      for (const child of task.tasks ?? []) visit(child);
    }
    for (const module of modules) visit(module.task);
    console.log('MUTATION_EXECUTION ' + JSON.stringify({
      unhandledErrors: unhandledErrors.map(error => ({ name: error.name, message: error.message })),
      suiteErrors,
      reason
    }));
  }
}
