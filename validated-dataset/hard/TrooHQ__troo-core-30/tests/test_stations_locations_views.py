import os
import uuid
from datetime import time

from django.test import override_settings
from django.urls import reverse

from apps.common.scope_map import SCOPE_MAP
from apps.common.tests.base_api_test import BaseAPITestCase
from apps.merchants.models import Location, LocationWorkingHours, Station


override_settings(
    DATABASES={
        "default": {
            "ENGINE": "django.db.backends.sqlite3",
            "NAME": ":memory:",
        }
    },
    CACHES={
        "default": {
            "BACKEND": "django.core.cache.backends.locmem.LocMemCache",
        }
    },
    CELERY_TASK_ALWAYS_EAGER=True,
    CELERY_TASK_EAGER_PROPAGATES=True,
    CELERY_BROKER_URL="memory://",
    CELERY_RESULT_BACKEND="cache+memory://",
)
class StationsLocationsViewsTestCase(BaseAPITestCase):
    def setUp(self):
        super().setUp()
        self.merchant = self.create_merchant()
        self.user = self.create_user()
        self.role = self.create_role(self.merchant, "Manager")
        self.link_user_to_merchant(self.user, self.merchant, self.role, is_owner=True)
        self.location = Location.objects.create(
            merchant=self.merchant,
            location_name="Main Hub",
            address_line1="123 Example St",
            address_line2="",
            city="Lagos",
            state="Lagos",
            postal_code="100001",
            country="NG",
            timezone="Africa/Lagos",
            is_closed=False,
            is_default=False,
            location_type="BRANCH",
        )

    def _scopes(self, *scope_keys):
        return [SCOPE_MAP[key] for key in scope_keys]

    def test_station_list_returns_location_object_and_create_requires_location_id(self):
        station = Station.objects.create(
            merchant=self.merchant,
            name="Desk 12",
            location=self.location,
            is_active=True,
        )
        url = reverse("stations-list")
        resp = self.get(
            url,
            scopes=self._scopes("merchants.station.read"),
            user=self.user,
        )
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertEqual(len(data), 1)
        self.assertEqual(data[0]["id"], str(station.id))
        self.assertIsInstance(data[0]["location"], dict)
        self.assertEqual(data[0]["location"]["id"], str(self.location.id))
        self.assertEqual(data[0]["location"]["name"], self.location.location_name)

        create_payload = {
            "name": "Desk 23",
            "location_id": str(self.location.id),
            "is_active": False,
        }
        create_resp = self.post(
            url,
            create_payload,
            scopes=self._scopes("merchants.station.create"),
            user=self.user,
        )
        self.assertEqual(create_resp.status_code, 201, create_resp.json())
        created = Station.objects.get(name="Desk 23")
        self.assertEqual(created.location_id, self.location.id)
        self.assertFalse(created.is_active)

    def test_station_create_rejects_missing_location_id(self):
        url = reverse("stations-list")
        resp = self.post(
            url,
            {"name": "Desk 99"},
            scopes=self._scopes("merchants.station.create"),
            user=self.user,
        )
        self.assertEqual(resp.status_code, 400)
        self.assertIn("location_id", resp.json())

    def test_working_hours_list_returns_location_object_and_create_uses_location_id(self):
        working = LocationWorkingHours.objects.create(
            location=self.location,
            day_of_week="MONDAY",
            open_time=time(8, 0),
            close_time=time(18, 0),
            is_closed=False,
        )
        url = reverse("working-hours-list")
        resp = self.get(
            url,
            scopes=self._scopes("merchants.location.working_hours.read"),
            user=self.user,
        )
        self.assertEqual(resp.status_code, 200)
        data = resp.json()
        self.assertEqual(len(data), 1)
        self.assertEqual(data[0]["id"], str(working.id))
        self.assertIsInstance(data[0]["location"], dict)
        self.assertEqual(data[0]["location"]["id"], str(self.location.id))
        self.assertEqual(data[0]["location"]["name"], self.location.location_name)

        create_payload = {
            "location_id": str(self.location.id),
            "day_of_week": "TUESDAY",
            "open_time": "09:00:00",
            "close_time": "17:00:00",
            "is_closed": False,
        }
        create_resp = self.post(
            url,
            create_payload,
            scopes=self._scopes("merchants.location.working_hours.create"),
            user=self.user,
        )
        self.assertEqual(create_resp.status_code, 201, create_resp.json())
        created = LocationWorkingHours.objects.get(day_of_week="TUESDAY")
        self.assertEqual(created.location_id, self.location.id)

    def test_working_hours_create_rejects_missing_location_id(self):
        url = reverse("working-hours-list")
        resp = self.post(
            url,
            {
                "day_of_week": "WEDNESDAY",
                "open_time": "10:00:00",
                "close_time": "16:00:00",
                "is_closed": False,
            },
            scopes=self._scopes("merchants.location.working_hours.create"),
            user=self.user,
        )
        self.assertEqual(resp.status_code, 400)
        self.assertIn("location_id", resp.json())
