package com.shyashyashya.refit.unit.interview.dto;

import static com.shyashyashya.refit.unit.fixture.CompanyFixture.TEST_COMPANY;
import static com.shyashyashya.refit.unit.fixture.JobCategoryFixture.TEST_JOB_CATEGORY;
import static com.shyashyashya.refit.unit.fixture.UserFixture.TEST_USER_1;
import static org.assertj.core.api.Assertions.assertThat;

import com.shyashyashya.refit.domain.industry.model.Industry;
import com.shyashyashya.refit.domain.interview.dto.InterviewDto;
import com.shyashyashya.refit.domain.interview.model.Interview;
import com.shyashyashya.refit.domain.interview.model.InterviewType;
import java.time.LocalDateTime;

import org.junit.jupiter.api.Test;

class InterviewDtoIndustryFieldsTest {

    @Test
    void interviewDtoShouldIncludeIndustryIdField() {
        Industry industry = Industry.create("Technology");
        Interview interview = Interview.create(
                "Engineer",
                InterviewType.TECHNICAL,
                LocalDateTime.of(2025, 1, 15, 10, 0),
                TEST_USER_1,
                TEST_COMPANY,
                industry,
                TEST_JOB_CATEGORY
        );
        InterviewDto dto = InterviewDto.from(interview);
        assertThat(dto.industryId()).isEqualTo(industry.getId());
    }

    @Test
    void interviewDtoShouldIncludeIndustryNameField() {
        Industry industry = Industry.create("Healthcare");
        Interview interview = Interview.create(
                "Doctor",
                InterviewType.BEHAVIORAL,
                LocalDateTime.of(2025, 2, 20, 14, 0),
                TEST_USER_1,
                TEST_COMPANY,
                industry,
                TEST_JOB_CATEGORY
        );
        InterviewDto dto = InterviewDto.from(interview);
        assertThat(dto.industryName()).isEqualTo("Healthcare");
    }

    @Test
    void industryFieldsShouldNotBeNull() {
        Industry industry = Industry.create("Finance");
        Interview interview = Interview.create(
                "Analyst",
                InterviewType.BEHAVIORAL,
                LocalDateTime.of(2025, 3, 1, 9, 0),
                TEST_USER_1,
                TEST_COMPANY,
                industry,
                TEST_JOB_CATEGORY
        );
        InterviewDto dto = InterviewDto.from(interview);
        assertThat(dto.industryId()).isNotNull();
        assertThat(dto.industryName()).isNotNull();
    }

    @Test
    void differentIndustriesShouldReturnDifferentInfo() {
        Industry retail = Industry.create("Retail");
        Industry education = Industry.create("Education");
        
        Interview interview1 = Interview.create(
                "Manager",
                InterviewType.BEHAVIORAL,
                LocalDateTime.of(2025, 4, 5, 11, 0),
                TEST_USER_1,
                TEST_COMPANY,
                retail,
                TEST_JOB_CATEGORY
        );
        
        Interview interview2 = Interview.create(
                "Teacher",
                InterviewType.BEHAVIORAL,
                LocalDateTime.of(2025, 5, 10, 13, 30),
                TEST_USER_1,
                TEST_COMPANY,
                education,
                TEST_JOB_CATEGORY
        );

        InterviewDto dto1 = InterviewDto.from(interview1);
        InterviewDto dto2 = InterviewDto.from(interview2);

        assertThat(dto1.industryId()).isEqualTo(retail.getId());
        assertThat(dto1.industryName()).isEqualTo("Retail");
        assertThat(dto2.industryId()).isEqualTo(education.getId());
        assertThat(dto2.industryName()).isEqualTo("Education");
        assertThat(dto1.industryId()).isNotEqualTo(dto2.industryId());
    }
}
