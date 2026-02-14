# openatv/enigma2-3706

openatv/enigma2 (#3706): [SoftCSA] Suppress audio in SoftDecoder for PIP to prevent audio routing corruption

Prevent picture-in-picture playback of CSA-ALT/SoftCSA channels from disrupting the main program’s audio. When a SoftDecoder is used for PIP, it must not open or route audio in a way that interferes with the primary audio stream, matching the behavior of hardware decoding where PIP audio is suppressed. Ensure switching PIP from SoftCSA to free-to-air no longer causes main audio loss.
