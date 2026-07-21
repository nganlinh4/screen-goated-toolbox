package dev.screengoated.toolbox.mobile.creation.worker;

import dev.screengoated.toolbox.mobile.creation.worker.ICreationWorkerCallback;

interface ICreationWorker {
    void prepare(ICreationWorkerCallback callback);
    void runJob(String requestJson, ICreationWorkerCallback callback);
    void cancel(String jobId);
}
