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

class InterviewDtoIndustryTest {

    @Test
    void shouldReturnCorrectIndustryName() {
        Industry industry = Industry.create("Manufacturing");
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
        assertThat(dto.industryName()).isEqualTo("Manufacturing");
    }

    @Test
    void shouldReturnCorrectIndustryId() {
        Industry industry = Industry.create("Finance");
        Interview interview = Interview.create(
                "Analyst",
                InterviewType.BEHAVIORAL,
                LocalDateTime.of(2025, 2, 20, 14, 0),
                TEST_USER_1,
                TEST_COMPANY,
                industry,
                TEST_JOB_CATEGORY
        );
        InterviewDto dto = InterviewDto.from(interview);
        assertThat(dto.industryId()).isEqualTo(industry.getId());
    }
}
