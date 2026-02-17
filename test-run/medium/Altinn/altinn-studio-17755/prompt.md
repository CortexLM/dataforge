# Altinn/altinn-studio-17755

Altinn/altinn-studio (#17755): refactor: Move PublishedElements class to the pure-functions library

Expose the PublishedElements utilities for use outside the content library so they can be used in the layout editor. Ensure the functionality remains pure and accessible from the pure-functions library, with no change to user-facing behavior.
