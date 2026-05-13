from django.db import models
from apps.core.models import HotelScopedModel
from apps.rooms.models import Room
from apps.guests.models import Guest
class Source(models.TextChoices): FRONT_DESK='FRONT_DESK'; BOOKING='BOOKING'; EXPEDIA='EXPEDIA'; WALK_IN='WALK_IN'
class Reservation(HotelScopedModel):
    guest=models.ForeignKey(Guest,on_delete=models.PROTECT)
    room=models.ForeignKey(Room,on_delete=models.PROTECT)
    check_in_date=models.DateField(); check_out_date=models.DateField()
    source=models.CharField(max_length=20, choices=Source.choices); external_ref=models.CharField(max_length=120, blank=True)
    status=models.CharField(max_length=20, default='CONFIRMED')
    total_amount=models.DecimalField(max_digits=12, decimal_places=2, default=0)
    is_paid=models.BooleanField(default=False)
    paid_amount=models.DecimalField(max_digits=12, decimal_places=2, default=0)
    is_occupied=models.BooleanField(default=False)
    check_in_time=models.TimeField(null=True, blank=True)
    check_out_time=models.TimeField(null=True, blank=True)
    actual_check_in=models.DateTimeField(null=True, blank=True)
    actual_check_out=models.DateTimeField(null=True, blank=True)
    
    def save(self, *args, **kwargs):
        # Auto-update is_paid flag
        total = float(self.total_amount or 0)
        paid = float(self.paid_amount or 0)
        if total > 0:
            self.is_paid = paid >= total
        else:
            self.is_paid = paid >= 0 # Handle complimentary if needed
        super().save(*args, **kwargs)

class ReservationGuest(HotelScopedModel):
    reservation=models.ForeignKey(Reservation, on_delete=models.CASCADE, related_name='guests')
    guest=models.ForeignKey(Guest, on_delete=models.CASCADE)
    is_primary=models.BooleanField(default=False)

class Transaction(HotelScopedModel):
    reservation=models.ForeignKey(Reservation, on_delete=models.CASCADE, related_name='transactions')
    amount_original=models.DecimalField(max_digits=12, decimal_places=2)
    currency=models.CharField(max_length=10, default='USD')
    conversion_rate=models.DecimalField(max_digits=10, decimal_places=4, default=1.0)
    amount_ref=models.DecimalField(max_digits=12, decimal_places=2) # Converted
    payment_type=models.CharField(max_length=40) # Cash, Card, Transfer
    date=models.DateField(auto_now_add=True)
    time=models.TimeField(auto_now_add=True)
