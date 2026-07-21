package dev.screengoated.toolbox.mobile.creation.worker;

oneway interface ICreationWorkerCallback {
    void onEvent(String eventJson);
}
