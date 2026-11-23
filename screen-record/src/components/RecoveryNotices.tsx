import { AudioImportRecoveryNotice } from "@/components/AudioImportRecoveryNotice";
import { SubtitleRecoveryNotice } from "@/components/SubtitleRecoveryNotice";

export default function RecoveryNotices({ ownerClipId }: { ownerClipId: string | null }) {
  return <><SubtitleRecoveryNotice /><AudioImportRecoveryNotice ownerClipId={ownerClipId} /></>;
}
