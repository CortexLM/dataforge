# omegat-org/segmentation-migrator-32

omegat-org/segmentation-migrator (#32): feat: extract SRX utility methods to new `SRXUtils` class, add XML serialization and deserialization tests

Extract reusable SRX-related utility behavior into a dedicated utility class and ensure SRX XML serialization and deserialization are covered by automated tests. The library should correctly read and write SRX XML, and the new utility access should not change external behavior.
