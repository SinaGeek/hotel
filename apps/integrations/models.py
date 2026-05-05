from django.db import models
from apps.core.models import HotelScopedModel

class IntegrationAccount(HotelScopedModel):
    provider=models.CharField(max_length=20); credentials=models.JSONField(default=dict)

class IntegrationEvent(HotelScopedModel):
    direction=models.CharField(max_length=20); status=models.CharField(max_length=20, default='pending')
    payload=models.JSONField(default=dict)

class DoorLockDevice(HotelScopedModel):
    name = models.CharField(max_length=100)
    room = models.OneToOneField('rooms.Room', on_delete=models.SET_NULL, null=True, blank=True)
    ip_address = models.GenericIPAddressField(null=True, blank=True)
    mac_address = models.CharField(max_length=50, null=True, blank=True)
    status = models.CharField(max_length=20, default='OFFLINE')

class PointOfSaleDevice(HotelScopedModel):
    name = models.CharField(max_length=100)
    device_type = models.CharField(max_length=50, choices=(('RESTAURANT', 'Restaurant POS'), ('RECEPTION', 'Reception POS')))
    ip_address = models.GenericIPAddressField(null=True, blank=True)
    status = models.CharField(max_length=20, default='OFFLINE')
