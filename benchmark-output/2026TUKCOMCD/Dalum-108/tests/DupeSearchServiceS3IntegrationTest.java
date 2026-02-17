package dalum.dalum.domain.dupe_product.service;

import dalum.dalum.global.s3.S3Service;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.lang.reflect.Constructor;
import java.lang.reflect.Field;

import static org.assertj.core.api.Assertions.*;

@DisplayName("DupeSearchService S3 통합 테스트")
class DupeSearchServiceS3IntegrationTest {

    @Test
    @DisplayName("DupeSearchService는 S3Service를 의존성으로 가져야 한다")
    void dupeSearchService_HasS3ServiceField() {
        // Verify that DupeSearchService has a field of type S3Service
        assertThat(DupeSearchService.class.getDeclaredFields())
            .anyMatch(field -> field.getType().equals(S3Service.class));
    }
    
    @Test
    @DisplayName("DupeSearchService는 S3Service를 주입받는 생성자를 가져야 한다")
    void dupeSearchService_HasConstructorWithS3Service() {
        // Verify constructor injection includes S3Service
        Constructor<?>[] constructors = DupeSearchService.class.getConstructors();
        
        assertThat(constructors)
            .anyMatch(constructor -> {
                Class<?>[] paramTypes = constructor.getParameterTypes();
                for (Class<?> paramType : paramTypes) {
                    if (paramType.equals(S3Service.class)) {
                        return true;
                    }
                }
                return false;
            });
    }
}
