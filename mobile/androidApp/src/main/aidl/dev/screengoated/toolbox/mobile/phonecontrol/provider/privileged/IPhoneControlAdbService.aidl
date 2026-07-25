package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged;

interface IPhoneControlAdbService {
    String connectAndVerify(long timeoutMs) = 1;
    String pairAndVerify(String pairingCode, long timeoutMs) = 2;
    String runCommand(String operationId, String program, in String[] args, String cwd, long timeoutMs) = 3;
    String cancelCommand(String operationId) = 4;
    String forget() = 5;
    String openBrowserTunnel(long timeoutMs) = 6;
    String closeBrowserTunnel(String leaseId) = 7;
}
