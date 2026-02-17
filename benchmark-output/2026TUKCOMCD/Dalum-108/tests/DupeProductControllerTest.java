package dalum.dalum.domain.dupe_product.controller;

import dalum.dalum.domain.dupe_product.dto.request.DupeSearchRequest;
import dalum.dalum.domain.dupe_product.service.DupeSearchService;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.io.IOException;
import java.lang.reflect.Method;
import java.util.Arrays;

import static org.assertj.core.api.Assertions.*;

@ExtendWith(MockitoExtension.class)
@DisplayName("DupeProductController API 테스트")
class DupeProductControllerTest {

    @Mock
    private DupeSearchService dupeSearchService;

    @InjectMocks
    private DupeProductController dupeProductController;

    @Test
    @DisplayName("searchDupe 메소드는 DupeSearchRequest를 파라미터로 받아야 한다")
    void searchDupe_AcceptsDupeSearchRequest() {
        // Verify the method exists with correct parameter type
        assertThat(DupeProductController.class.getMethods())
            .anyMatch(m -> m.getName().equals("searchDupe") && 
                          m.getParameterCount() == 1 &&
                          m.getParameterTypes()[0].equals(DupeSearchRequest.class));
    }

    @Test
    @DisplayName("DupeProductController는 DupeSearchService를 의존성으로 가져야 한다")
    void controller_HasDupeSearchServiceField() {
        // Verify that DupeProductController has a field of type DupeSearchService
        assertThat(DupeProductController.class.getDeclaredFields())
            .anyMatch(field -> field.getType().equals(DupeSearchService.class));
    }
    
    @Test
    @DisplayName("Controller 클래스는 @RestController 어노테이션을 가져야 한다")
    void controller_IsRestController() {
        assertThat(DupeProductController.class.isAnnotationPresent(org.springframework.web.bind.annotation.RestController.class))
            .isTrue();
    }
}
