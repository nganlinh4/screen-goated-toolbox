package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged;

interface IPhoneControlShellService {
    void destroy() = 16777114;
    String runCommand(String operationId, String program, in String[] args, String cwd, long timeoutMs) = 1;
    String cancelCommand(String operationId) = 2;
}
