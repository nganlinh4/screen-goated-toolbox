package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupNavigationContract

internal val WirelessDebuggingSetupContract = PhoneControlProtectedSetupNavigationContract(
    platformCapability = "Android secure wireless debugging",
    destinationState =
        "the system-owned pairing-by-code surface showing one ephemeral code and its local endpoint",
)
