# cluesmith/codev-371

cluesmith/codev (#371): [Bugfix #370] Fix Gemini --yolo mode in general consultations

Prevent Gemini general consultations from receiving elevated file-write access. Ensure the general mode invocation does not pass any flag that enables auto-approval or write permissions, while protocol/typed consultations still include the flag needed for structured reviews with file access. Update behavior so only protocol mode gets write access and general mode remains read-only.
