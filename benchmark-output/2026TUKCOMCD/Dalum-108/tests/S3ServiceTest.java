package dalum.dalum.global.s3;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;

import static org.assertj.core.api.Assertions.*;

@DisplayName("S3Service API 테스트")
class S3ServiceTest {

    @Test
    @DisplayName("S3Service 클래스가 존재해야 한다")
    void s3Service_ClassExists() {
        // Verify the S3Service class exists
        assertThatCode(() -> Class.forName("dalum.dalum.global.s3.S3Service"))
            .doesNotThrowAnyException();
    }

    @Test
    @DisplayName("S3Service는 uploadFile 메소드를 가지고 있어야 한다")
    void s3Service_HasUploadFileMethod() throws NoSuchMethodException {
        Class<?> clazz = S3Service.class;
        Method method = clazz.getMethod("uploadFile", org.springframework.web.multipart.MultipartFile.class);
        assertThat(method).isNotNull();
        assertThat(method.getExceptionTypes()).contains(IOException.class);
    }
    
    @Test
    @DisplayName("S3Service는 deleteFile 메소드를 가지고 있어야 한다")
    void s3Service_HasDeleteFileMethod() throws NoSuchMethodException {
        Class<?> clazz = S3Service.class;
        Method method = clazz.getMethod("deleteFile", String.class);
        assertThat(method).isNotNull();
    }
}
