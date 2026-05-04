from django.db import models
from apps.core.models import HotelScopedModel
class IntegrationAccount(HotelScopedModel):
    provider=models.CharField(max_length=20); credentials=models.JSONField(default=dict)
class IntegrationEvent(HotelScopedModel):
    direction=models.CharField(max_length=20); status=models.CharField(max_length=20, default='pending')
    payload=models.JSONField(default=dict)
