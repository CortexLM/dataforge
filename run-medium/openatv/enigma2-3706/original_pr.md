# openatv/enigma2-3706 (original PR)

openatv/enigma2 (#3706): [SoftCSA] Suppress audio in SoftDecoder for PIP to prevent audio routing corruption

When a CSA-ALT channel runs as PIP, the SoftDecoder unconditionally called setAudioPID(), opening audio1 and interfering with the main picture's audio0 routing. This caused reproducible audio loss when switching PIP from a SoftCSA channel to FreeTV.

Add m_noaudio flag to eDVBSoftDecoder, matching the existing guard in the hardware decoder path (eDVBServicePlay::selectAudioStream).
